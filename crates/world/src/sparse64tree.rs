use crate::voxel_grid::VoxelGrid;
use capy_core::{MATERIAL_COLORS, MATERIAL_PALETTE_SIZE, MaterialId, is_water_material};

const BRANCH: u32 = 4;
const BRANCH_CUBED: usize = (BRANCH * BRANCH * BRANCH) as usize;
pub(crate) const NODE_FLAG_LEAF: u32 = 1;
pub(crate) const NODE_FLAG_UNIFORM_WATER: u32 = 1 << 1;

pub(crate) fn local_to_bit(lx: u32, ly: u32, lz: u32) -> u32 {
    lx + ly * BRANCH + lz * BRANCH * BRANCH
}

pub(crate) fn leaf_node_flags(mask: u64, materials: &[MaterialId; BRANCH_CUBED]) -> u32 {
    let mut flags = NODE_FLAG_LEAF;
    if mask == u64::MAX && materials.iter().all(|&mat| is_water_material(mat)) {
        flags |= NODE_FLAG_UNIFORM_WATER;
    }
    flags
}

pub(crate) fn inner_node_flags(mask: u64, child_flags: &[u32]) -> u32 {
    if mask == u64::MAX
        && child_flags.len() == BRANCH_CUBED
        && child_flags
            .iter()
            .all(|&flags| (flags & NODE_FLAG_UNIFORM_WATER) != 0)
    {
        NODE_FLAG_UNIFORM_WATER
    } else {
        0
    }
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

/// Cached occupancy state for incremental chunk baking.
/// Maintains a multi-level occupancy mip that tracks which 4×4×4 blocks
/// contain voxels. Can be updated incrementally for small edits instead
/// of rescanning the entire grid.
#[derive(Clone)]
pub struct ChunkOccupancy {
    mip: OccupancyMip,
    world_size: u32,
    depth: u32,
}

impl ChunkOccupancy {
    /// Build initial occupancy from a voxel grid.
    pub fn build(grid: &VoxelGrid, col_heights: Option<&[u16]>) -> Self {
        let max_dim = grid.size_x.max(grid.size_y).max(grid.size_z);
        let world_size = next_power_of_4(max_dim);
        let depth = depth_for_size(max_dim);
        let mip = OccupancyMip::build(grid, col_heights, world_size, depth);
        Self {
            mip,
            world_size,
            depth,
        }
    }

    /// Update occupancy for the region affected by an edit.
    /// `min` and `max` define an axis-aligned bounding box in voxel coordinates (max is exclusive).
    pub fn update(&mut self, grid: &VoxelGrid, min: [i32; 3], max: [i32; 3]) {
        let leaf_size = BRANCH;
        let blocks_x = self.mip.blocks_x;
        let blocks_y = self.mip.blocks_y;
        let blocks_z = self.mip.blocks_z;

        // Clamp to grid bounds and compute affected block range
        let bx_min = (min[0].max(0) as u32 / leaf_size).min(blocks_x);
        let by_min = (min[1].max(0) as u32 / leaf_size).min(blocks_y);
        let bz_min = (min[2].max(0) as u32 / leaf_size).min(blocks_z);
        let bx_max = (max[0].max(0) as u32).div_ceil(leaf_size).min(blocks_x);
        let by_max = (max[1].max(0) as u32).div_ceil(leaf_size).min(blocks_y);
        let bz_max = (max[2].max(0) as u32).div_ceil(leaf_size).min(blocks_z);

        // Update L0: re-check each affected leaf block
        for bz in bz_min..bz_max {
            for by in by_min..by_max {
                for bx in bx_min..bx_max {
                    let ox = bx * leaf_size;
                    let oy = by * leaf_size;
                    let oz = bz * leaf_size;
                    let (mask, _) = grid.read_leaf_block(ox, oy, oz);
                    let idx = (bx + by * blocks_x + bz * blocks_x * blocks_y) as usize;
                    self.mip.levels[0][idx] = mask != 0;
                }
            }
        }

        // Propagate to higher mip levels
        let mut prev_bx = blocks_x;
        let mut prev_by = blocks_y;
        let mut prev_bz = blocks_z;
        let mut prev_min = [bx_min, by_min, bz_min];
        let mut prev_max = [bx_max, by_max, bz_max];

        for level in 1..self.mip.levels.len() {
            let cur_bx = prev_bx / BRANCH;
            let cur_by = prev_by / BRANCH;
            let cur_bz = prev_bz / BRANCH;
            if cur_bx == 0 || cur_by == 0 || cur_bz == 0 {
                break;
            }

            // Block range at this level that could be affected
            let gx_min = prev_min[0] / BRANCH;
            let gy_min = prev_min[1] / BRANCH;
            let gz_min = prev_min[2] / BRANCH;
            let gx_max = prev_max[0].div_ceil(BRANCH).min(cur_bx);
            let gy_max = prev_max[1].div_ceil(BRANCH).min(cur_by);
            let gz_max = prev_max[2].div_ceil(BRANCH).min(cur_bz);

            let (prev_levels, cur_levels) = self.mip.levels.split_at_mut(level);
            let prev_level = &prev_levels[level - 1];
            let cur_level = &mut cur_levels[0];
            for gz in gz_min..gz_max {
                for gy in gy_min..gy_max {
                    for gx in gx_min..gx_max {
                        let mut occupied = false;
                        'scan: for lz in 0..BRANCH {
                            for ly in 0..BRANCH {
                                for lx in 0..BRANCH {
                                    let px = gx * BRANCH + lx;
                                    let py = gy * BRANCH + ly;
                                    let pz = gz * BRANCH + lz;
                                    let idx = (px + py * prev_bx + pz * prev_bx * prev_by) as usize;
                                    if idx < prev_level.len() && prev_level[idx] {
                                        occupied = true;
                                        break 'scan;
                                    }
                                }
                            }
                        }
                        cur_level[(gx + gy * cur_bx + gz * cur_bx * cur_by) as usize] = occupied;
                    }
                }
            }

            prev_bx = cur_bx;
            prev_by = cur_by;
            prev_bz = cur_bz;
            prev_min = [gx_min, gy_min, gz_min];
            prev_max = [gx_max, gy_max, gz_max];
        }
    }
}

/// Build a flat tree using a pre-built occupancy mip (skips the mip scan).
pub(crate) fn build_tree_from_mip(grid: &VoxelGrid, occ: &ChunkOccupancy) -> FlatTree {
    let mut builder = TreeBuilder {
        grid,
        occ: &occ.mip,
        buffer: Vec::new(),
        avg_color_buf: Vec::new(),
    };

    let root_offset = builder.build_subtree([0, 0, 0], occ.world_size, occ.depth);

    FlatTree {
        buffer: builder.buffer,
        avg_color_buf: builder.avg_color_buf,
        root_offset,
        world_size: occ.world_size,
        depth: occ.depth,
    }
}

#[derive(Clone)]
struct OccupancyMip {
    levels: Vec<Vec<bool>>,
    blocks_x: u32,
    blocks_y: u32,
    blocks_z: u32,
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
                    if ox >= grid.size_x || oz >= grid.size_z {
                        continue;
                    }
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
            // Single sequential pass over grid data — much more cache-friendly
            // than per-block random access via volume_has_voxels_direct.
            let sx = grid.size_x as usize;
            let sy = grid.size_y as usize;
            let sz = grid.size_z as usize;
            let sxy = sx * sy;
            let ls = leaf_size as usize;
            let bx_count = blocks_x as usize;
            let bxy_count = (blocks_x * blocks_y) as usize;
            let mut l0 = vec![false; total_blocks];

            for z in 0..sz {
                let bz = z / ls;
                let z_offset = z * sxy;
                for y in 0..sy {
                    let by = y / ls;
                    let row_start = z_offset + y * sx;
                    let block_yz = by * bx_count + bz * bxy_count;
                    for bx in 0..bx_count {
                        if l0[bx + block_yz] {
                            continue;
                        }
                        let chunk_start = row_start + bx * ls;
                        let chunk_end = (chunk_start + ls).min(row_start + sx);
                        if grid.data[chunk_start..chunk_end].iter().any(|&v| v != 0) {
                            l0[bx + block_yz] = true;
                        }
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

            let (mask, materials) = self.grid.read_leaf_block(ox, oy, oz);

            let avg_color = compute_leaf_avg_color(&materials, mask);

            self.buffer.push(mask as u32);
            self.buffer.push((mask >> 32) as u32);
            self.buffer.push(leaf_node_flags(mask, &materials));

            for chunk in materials.chunks(2) {
                let first = chunk[0] as u32;
                let second = chunk[1] as u32;
                self.buffer.push(first | (second << 16));
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
        let mut child_flags = Vec::with_capacity(child_count);
        for (i, &child_origin) in child_positions.iter().enumerate() {
            let child_offset = self.build_subtree(child_origin, child_size, remaining_depth - 1);
            self.buffer[ptrs_start + i] = child_offset;
            child_flags.push(self.buffer[child_offset as usize + 2]);

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

        self.buffer[offset as usize + 2] = inner_node_flags(mask, &child_flags);

        offset
    }
}

pub(crate) fn compute_leaf_avg_color(materials: &[MaterialId; BRANCH_CUBED], mask: u64) -> [u8; 3] {
    let occupied = mask.count_ones() as usize;
    if occupied == 0 {
        return [0, 0, 0];
    }
    let mut sum = [0.0f32; 3];
    for (bit, &mat_id) in materials.iter().enumerate().take(BRANCH_CUBED) {
        if (mask & (1u64 << bit)) != 0 {
            let mat = capy_core::visual_material(mat_id) as usize;
            if mat < MATERIAL_PALETTE_SIZE {
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

pub(crate) fn tree_to_gpu_data(flat: FlatTree) -> (TreeInfoUniform, Vec<u32>, Vec<u32>) {
    let info = TreeInfoUniform {
        world_size: flat.world_size,
        root_offset: flat.root_offset,
        depth: flat.depth,
        _pad: 0,
    };
    let buf_len = flat.buffer.len();
    let mut avg_color = flat.avg_color_buf;
    avg_color.resize(buf_len, 0);
    (info, flat.buffer, avg_color)
}

#[cfg(test)]
mod tests {
    use super::{
        NODE_FLAG_LEAF, NODE_FLAG_UNIFORM_WATER, build_and_serialize_tree_with_heights,
        build_tree_from_mip,
    };
    use crate::voxel_grid::VoxelGrid;
    use capy_core::WATER_BIT;

    const WATER_MATERIAL: u16 = 8 | WATER_BIT;

    fn uniform_grid(size: u32, material: u16) -> VoxelGrid {
        VoxelGrid::new(
            size,
            size,
            size,
            vec![material; (size * size * size) as usize],
        )
        .expect("grid dimensions should match data length")
    }

    #[test]
    fn full_water_leaf_sets_uniform_water_flag() {
        let grid = uniform_grid(4, WATER_MATERIAL);
        let flat = build_and_serialize_tree_with_heights(&grid, None);

        assert_eq!(flat.depth, 1);
        assert_eq!(
            flat.buffer[flat.root_offset as usize + 2],
            NODE_FLAG_LEAF | NODE_FLAG_UNIFORM_WATER
        );
    }

    #[test]
    fn full_water_inner_node_sets_uniform_water_flag() {
        let grid = uniform_grid(16, WATER_MATERIAL);
        let flat = build_and_serialize_tree_with_heights(&grid, None);

        assert_eq!(flat.depth, 2);
        assert_ne!(
            flat.buffer[flat.root_offset as usize + 2] & NODE_FLAG_UNIFORM_WATER,
            0
        );
    }

    #[test]
    fn partial_water_leaf_does_not_set_uniform_water_flag() {
        let mut grid = uniform_grid(4, WATER_MATERIAL);
        grid.set(0, 0, 0, 0);
        let flat = build_and_serialize_tree_with_heights(&grid, None);

        assert_eq!(flat.depth, 1);
        assert_eq!(
            flat.buffer[flat.root_offset as usize + 2] & NODE_FLAG_UNIFORM_WATER,
            0
        );
    }

    #[test]
    fn incremental_build_keeps_uniform_water_flag() {
        let grid = uniform_grid(16, WATER_MATERIAL);
        let occ = crate::sparse64tree::ChunkOccupancy::build(&grid, None);
        let flat = build_tree_from_mip(&grid, &occ);

        assert_ne!(
            flat.buffer[flat.root_offset as usize + 2] & NODE_FLAG_UNIFORM_WATER,
            0
        );
    }
}
