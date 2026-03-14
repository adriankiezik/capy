use crate::material::MATERIAL_COLORS;
use crate::voxel_grid::VoxelGrid;

const BRANCH: u32 = 4;
const BRANCH_CUBED: usize = (BRANCH * BRANCH * BRANCH) as usize;

fn local_to_bit(lx: u32, ly: u32, lz: u32) -> u32 {
    lx + ly * BRANCH + lz * BRANCH * BRANCH
}

fn next_power_of_4(size: u32) -> u32 {
    if size <= 1 {
        return 1;
    }
    let mut p = 1u32;
    while p < size {
        p *= 4;
    }
    p
}

fn depth_for_size(size: u32) -> u32 {
    let p = next_power_of_4(size);
    let mut d = 0u32;
    let mut s = 1u32;
    while s < p {
        s *= 4;
        d += 1;
    }
    d.max(1)
}

pub(crate) fn build_and_serialize_tree_with_heights(
    grid: &VoxelGrid,
    col_heights: Option<&[u16]>,
) -> FlatTree {
    let max_dim = grid.size_x.max(grid.size_y).max(grid.size_z);
    let world_size = next_power_of_4(max_dim);
    let depth = depth_for_size(max_dim);

    let occ = OccupancyMip::build(grid, col_heights, world_size, depth);

    let mut builder = TreeBuilder {
        grid,
        occ: &occ,
        buffer: Vec::new(),
        avg_color_buf: Vec::new(),
    };

    let root_offset = builder.build_subtree([0, 0, 0], world_size, depth);

    FlatTree {
        buffer: builder.buffer,
        avg_color_buf: builder.avg_color_buf,
        root_offset,
        world_size,
        depth,
    }
}

struct OccupancyMip {
    levels: Vec<Vec<bool>>,
    blocks_x: u32,
    blocks_y: u32,
    #[allow(dead_code)]
    blocks_z: u32,
    #[allow(dead_code)]
    depth: u32,
}

impl OccupancyMip {
    fn build(grid: &VoxelGrid, col_heights: Option<&[u16]>, world_size: u32, depth: u32) -> Self {
        let leaf_size = BRANCH;
        let blocks_x = world_size / leaf_size;
        let blocks_y = world_size / leaf_size;
        let blocks_z = world_size / leaf_size;
        let total_blocks = (blocks_x * blocks_y * blocks_z) as usize;

        let level0 = if let Some(heights) = col_heights {
            let cs = grid.size_x as usize;
            let mut l0 = vec![false; total_blocks];
            for bz in 0..blocks_z {
                for bx in 0..blocks_x {
                    let ox = bx * leaf_size;
                    let oz = bz * leaf_size;
                    let mut max_h = 0u16;
                    for lz in 0..leaf_size.min(grid.size_z - oz) {
                        for lx in 0..leaf_size.min(grid.size_x - ox) {
                            let idx = (ox + lx) as usize + (oz + lz) as usize * cs;
                            if idx < heights.len() {
                                max_h = max_h.max(heights[idx]);
                            }
                        }
                    }
                    for by in 0..blocks_y {
                        let oy = by * leaf_size;
                        let has = oy < max_h as u32;
                        l0[(bx + by * blocks_x + bz * blocks_x * blocks_y) as usize] = has;
                    }
                }
            }
            l0
        } else {
            let mut l0 = vec![false; total_blocks];
            for bz in 0..blocks_z {
                for by in 0..blocks_y {
                    for bx in 0..blocks_x {
                        let ox = bx * leaf_size;
                        let oy = by * leaf_size;
                        let oz = bz * leaf_size;
                        let has = volume_has_voxels_direct(grid, ox, oy, oz, leaf_size);
                        l0[(bx + by * blocks_x + bz * blocks_x * blocks_y) as usize] = has;
                    }
                }
            }
            l0
        };

        let mut levels: Vec<Vec<bool>> = vec![level0];

        let mut prev_bx = blocks_x;
        let mut prev_by = blocks_y;
        let mut prev_bz = blocks_z;

        for _ in 1..depth {
            let cur_bx = prev_bx / BRANCH;
            let cur_by = prev_by / BRANCH;
            let cur_bz = prev_bz / BRANCH;
            if cur_bx == 0 || cur_by == 0 || cur_bz == 0 {
                break;
            }
            let Some(prev) = levels.last() else {
                break;
            };
            let total = (cur_bx * cur_by * cur_bz) as usize;
            let mut cur = vec![false; total];

            for gz in 0..cur_bz {
                for gy in 0..cur_by {
                    for gx in 0..cur_bx {
                        let mut occupied = false;
                        'scan: for lz in 0..BRANCH {
                            for ly in 0..BRANCH {
                                for lx in 0..BRANCH {
                                    let px = gx * BRANCH + lx;
                                    let py = gy * BRANCH + ly;
                                    let pz = gz * BRANCH + lz;
                                    let idx = (px + py * prev_bx + pz * prev_bx * prev_by) as usize;
                                    if prev[idx] {
                                        occupied = true;
                                        break 'scan;
                                    }
                                }
                            }
                        }
                        cur[(gx + gy * cur_bx + gz * cur_bx * cur_by) as usize] = occupied;
                    }
                }
            }

            prev_bx = cur_bx;
            prev_by = cur_by;
            prev_bz = cur_bz;
            levels.push(cur);
        }

        debug_assert!(
            !levels.is_empty(),
            "OccupancyMip must have at least one level"
        );

        Self {
            levels,
            blocks_x,
            blocks_y,
            blocks_z,
            depth,
        }
    }

    fn has_voxels(&self, ox: u32, oy: u32, oz: u32, node_size: u32, remaining_depth: u32) -> bool {
        let level_idx = (remaining_depth as usize).saturating_sub(1);

        if level_idx >= self.levels.len() {
            return self
                .levels
                .last()
                .and_then(|l| l.first())
                .copied()
                .unwrap_or(false);
        }

        let level = &self.levels[level_idx];
        let bx = ox / node_size;
        let by = oy / node_size;
        let bz = oz / node_size;

        let divider = BRANCH.pow(level_idx as u32);
        let lbx = self.blocks_x / divider;
        let lby = self.blocks_y / divider;

        let idx = (bx + by * lbx + bz * lbx * lby) as usize;
        if idx < level.len() { level[idx] } else { false }
    }
}

fn volume_has_voxels_direct(grid: &VoxelGrid, ox: u32, oy: u32, oz: u32, size: u32) -> bool {
    let max_x = (ox + size).min(grid.size_x);
    let max_y = (oy + size).min(grid.size_y);
    let max_z = (oz + size).min(grid.size_z);

    for z in oz..max_z {
        for y in oy..max_y {
            for x in ox..max_x {
                if grid.get(x as i32, y as i32, z as i32) != 0 {
                    return true;
                }
            }
        }
    }
    false
}

struct TreeBuilder<'a> {
    grid: &'a VoxelGrid,
    occ: &'a OccupancyMip,
    buffer: Vec<u32>,
    avg_color_buf: Vec<u32>,
}

impl TreeBuilder<'_> {
    fn build_subtree(&mut self, origin: [u32; 3], node_size: u32, remaining_depth: u32) -> u32 {
        let [ox, oy, oz] = origin;

        if remaining_depth == 1 {
            let offset = self.buffer.len() as u32;

            let mut mask = 0u64;
            let mut materials = [0u8; BRANCH_CUBED];

            for lz in 0..BRANCH {
                for ly in 0..BRANCH {
                    for lx in 0..BRANCH {
                        let wx = ox + lx;
                        let wy = oy + ly;
                        let wz = oz + lz;
                        let mat = self.grid.get(wx as i32, wy as i32, wz as i32);
                        if mat != 0 {
                            let bit = local_to_bit(lx, ly, lz);
                            mask |= 1u64 << bit;
                            materials[bit as usize] = mat;
                        }
                    }
                }
            }

            let avg_color = compute_leaf_avg_color(&materials, mask);

            self.buffer.push(mask as u32);
            self.buffer.push((mask >> 32) as u32);
            self.buffer.push(1);

            for chunk in materials.chunks(4) {
                let word = u32::from_le_bytes([
                    chunk[0],
                    if chunk.len() > 1 { chunk[1] } else { 0 },
                    if chunk.len() > 2 { chunk[2] } else { 0 },
                    if chunk.len() > 3 { chunk[3] } else { 0 },
                ]);
                self.buffer.push(word);
            }

            self.avg_color_buf
                .resize(self.buffer.len().max(offset as usize + 1), 0);
            let avg_word = (avg_color[0] as u32)
                | ((avg_color[1] as u32) << 8)
                | ((avg_color[2] as u32) << 16);
            self.avg_color_buf[offset as usize] = avg_word;

            return offset;
        }

        let child_size = node_size / BRANCH;

        let mut mask = 0u64;
        let mut child_positions: Vec<[u32; 3]> = Vec::new();

        for lz in 0..BRANCH {
            for ly in 0..BRANCH {
                for lx in 0..BRANCH {
                    let cx = ox + lx * child_size;
                    let cy = oy + ly * child_size;
                    let cz = oz + lz * child_size;

                    if self
                        .occ
                        .has_voxels(cx, cy, cz, child_size, remaining_depth - 1)
                    {
                        let bit = local_to_bit(lx, ly, lz);
                        mask |= 1u64 << bit;
                        child_positions.push([cx, cy, cz]);
                    }
                }
            }
        }

        let child_count = child_positions.len();

        let offset = self.buffer.len() as u32;
        self.buffer.push(mask as u32);
        self.buffer.push((mask >> 32) as u32);
        self.buffer.push(0);

        let ptrs_start = self.buffer.len();
        self.buffer.resize(ptrs_start + child_count, 0);

        self.avg_color_buf
            .resize(self.buffer.len().max(offset as usize + 1), 0);

        let mut color_sum = [0.0f32; 3];
        for (i, &child_origin) in child_positions.iter().enumerate() {
            let child_offset = self.build_subtree(child_origin, child_size, remaining_depth - 1);
            self.buffer[ptrs_start + i] = child_offset;

            let child_avg = self
                .avg_color_buf
                .get(child_offset as usize)
                .copied()
                .unwrap_or(0);
            color_sum[0] += (child_avg & 0xFF) as f32;
            color_sum[1] += ((child_avg >> 8) & 0xFF) as f32;
            color_sum[2] += ((child_avg >> 16) & 0xFF) as f32;
        }

        if child_count > 0 {
            let n = child_count as f32;
            let avg_word = ((color_sum[0] / n).round() as u32)
                | (((color_sum[1] / n).round() as u32) << 8)
                | (((color_sum[2] / n).round() as u32) << 16);
            self.avg_color_buf[offset as usize] = avg_word;
        }

        offset
    }
}

fn compute_leaf_avg_color(materials: &[u8; BRANCH_CUBED], mask: u64) -> [u8; 3] {
    let occupied = mask.count_ones() as usize;
    if occupied == 0 {
        return [0, 0, 0];
    }
    let mut sum = [0.0f32; 3];
    for (bit, &mat_id) in materials.iter().enumerate().take(BRANCH_CUBED) {
        if (mask & (1u64 << bit)) != 0 {
            let mat = mat_id as usize;
            if mat < MATERIAL_COLORS.len() {
                sum[0] += MATERIAL_COLORS[mat][0];
                sum[1] += MATERIAL_COLORS[mat][1];
                sum[2] += MATERIAL_COLORS[mat][2];
            }
        }
    }
    let n = occupied as f32;
    [
        (sum[0] / n * 255.0).round() as u8,
        (sum[1] / n * 255.0).round() as u8,
        (sum[2] / n * 255.0).round() as u8,
    ]
}

#[derive(Debug, Clone)]
pub(crate) struct FlatTree {
    pub(crate) buffer: Vec<u32>,
    pub(crate) avg_color_buf: Vec<u32>,
    pub(crate) root_offset: u32,
    pub(crate) world_size: u32,
    pub(crate) depth: u32,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct TreeInfoUniform {
    pub world_size: u32,
    pub root_offset: u32,
    pub depth: u32,
    pub _pad: u32,
}

pub(crate) fn tree_to_gpu_data(flat: &FlatTree) -> (TreeInfoUniform, Vec<u32>, Vec<u32>) {
    let info = TreeInfoUniform {
        world_size: flat.world_size,
        root_offset: flat.root_offset,
        depth: flat.depth,
        _pad: 0,
    };
    let mut avg_color = flat.avg_color_buf.clone();
    avg_color.resize(flat.buffer.len(), 0);
    (info, flat.buffer.clone(), avg_color)
}
