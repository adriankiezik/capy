use std::collections::HashMap;

use bevy_ecs::resource::Resource;

use crate::{BakedChunkData, MATERIAL_PALETTE_SIZE, MaterialId, is_water_material};

/// Number of u32 words per slot in the indirection table.
/// 16 words = 64 bytes, so each slot occupies exactly one GPU cache line
/// (word 15 stores the near-mesh flags).
const INDIRECTION_STRIDE: usize = 16;
const SLOT_WORLD_SIZE: usize = 0;
const SLOT_ROOT_OFFSET: usize = 1;
const SLOT_DEPTH: usize = 2;
const SLOT_POOL_OFFSET: usize = 3;
const SLOT_FOLIAGE_Y_MIN: usize = 4;
const SLOT_FOLIAGE_Y_MAX: usize = 5;
const SLOT_FOLIAGE_BITMAP_OFFSET: usize = 6;
const SLOT_FOLIAGE_Y_BANDS: usize = 7;
const SLOT_FOLIAGE_TILE_Y_RANGES_OFFSET: usize = 8;
const SLOT_SOLID_MIN_X: usize = 9;
const SLOT_SOLID_MIN_Y: usize = 10;
const SLOT_SOLID_MIN_Z: usize = 11;
const SLOT_SOLID_MAX_X: usize = 12;
const SLOT_SOLID_MAX_Y: usize = 13;
const SLOT_SOLID_MAX_Z: usize = 14;
const SLOT_FLAGS: usize = 15;
const SLOT_FLAG_NEAR_MESH: u32 = 1;
const TREE_BRANCH: u32 = 4;
const NODE_FLAG_LEAF: u32 = 1;
const NODE_FLAG_UNIFORM_WATER: u32 = 1 << 1;

/// Tag bits embedded in the high bits of child-pointer words in the GPU pool.
/// They mirror the child node's header flags so GPU traversal can decide
/// leaf / uniform-water without a dependent read of the child header.
/// Tags exist only in the assembled GPU pool (`pool_dag`); the per-chunk
/// `BakedChunkData::dag_buffer` format on disk stays untagged.
pub const POOL_CHILD_FLAG_LEAF: u32 = 1 << 31;
pub const POOL_CHILD_FLAG_UNIFORM_WATER: u32 = 1 << 30;
pub const POOL_CHILD_OFFSET_MASK: u32 = 0x3FFF_FFFF;

/// OR each child pointer in a chunk's DAG region with the child's header
/// flags (leaf / uniform-water) in the top two bits. `region` is the
/// chunk-local DAG slice; child offsets are relative to its start.
/// Idempotent: already-tagged pointers are masked before following.
pub fn tag_pool_child_pointers(region: &mut [u32], root_offset: u32) {
    let root = root_offset as usize;
    if root + 3 > region.len() {
        return;
    }
    let mut visited = std::collections::HashSet::new();
    let mut stack = vec![root];
    while let Some(o) = stack.pop() {
        if !visited.insert(o) || o + 3 > region.len() {
            continue;
        }
        let flags = region[o + 2];
        if (flags & NODE_FLAG_LEAF) != 0 {
            continue;
        }
        let child_count = (region[o].count_ones() + region[o + 1].count_ones()) as usize;
        for i in 0..child_count {
            let Some(&ptr) = region.get(o + 3 + i) else {
                break;
            };
            let child = (ptr & POOL_CHILD_OFFSET_MASK) as usize;
            let child_flags = region.get(child + 2).copied().unwrap_or(0);
            let mut tagged = child as u32;
            if (child_flags & NODE_FLAG_LEAF) != 0 {
                tagged |= POOL_CHILD_FLAG_LEAF;
            }
            if (child_flags & NODE_FLAG_UNIFORM_WATER) != 0 {
                tagged |= POOL_CHILD_FLAG_UNIFORM_WATER;
            }
            region[o + 3 + i] = tagged;
            if (child_flags & NODE_FLAG_LEAF) == 0 {
                stack.push(child);
            }
        }
    }
}

/// Sentinel value: chunk has foliage but no bitmap (all columns are foliage).
const FOLIAGE_BITMAP_ALL: u32 = 0xFFFFFFFE;
/// Sentinel value: chunk has no foliage at all.
const FOLIAGE_BITMAP_NONE: u32 = 0xFFFFFFFF;
/// Sentinel value: no per-tile Y-range data (uniform foliage or no foliage).
const FOLIAGE_TILE_NONE: u32 = 0xFFFFFFFF;
const MAX_CHILD_FRACTION: f32 = 0.999_999_94;

#[derive(Clone, Copy)]
struct SlotTreeInfo {
    world_size: u32,
    root_offset: u32,
    depth: u32,
    pool_offset: u32,
}

/// CPU-side surface vertex used by the experimental near-field mesh path.
///
/// The renderer uploads these vertices directly when a voxel scene is rebuilt.
/// Keeping the prototype mesh next to the DAG data makes both representations
/// switch atomically after an editor rebake.
#[derive(Clone, Copy, Debug)]
pub struct VoxelSurfaceVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub material: MaterialId,
}

#[derive(Clone, Debug, Default)]
pub struct NearVoxelMeshData {
    pub vertices: Vec<VoxelSurfaceVertex>,
    pub indices: Vec<u32>,
    /// Independently drawable chunk ranges in `indices`.
    pub chunks: Vec<NearVoxelMeshChunk>,
    /// One local-space canonical chunk mesh, drawn through instancing.
    pub canonical_index_start: u32,
    pub canonical_index_count: u32,
    /// Unedited chunk slots represented by the canonical instance mesh.
    pub canonical_chunks: Vec<[i32; 3]>,
}

#[derive(Clone, Copy, Debug)]
pub struct NearVoxelMeshChunk {
    pub coord: [i32; 3],
    pub index_start: u32,
    pub index_count: u32,
}

fn bit_is_set_64(lo: u32, hi: u32, bit: u32) -> bool {
    if bit < 32 {
        (lo & (1u32 << bit)) != 0
    } else {
        (hi & (1u32 << (bit - 32))) != 0
    }
}

fn popcount_below(lo: u32, hi: u32, bit: u32) -> u32 {
    if bit < 32 {
        let mask = if bit == 0 { 0 } else { (1u32 << bit) - 1 };
        (lo & mask).count_ones()
    } else {
        let hi_bits = bit - 32;
        let hi_mask = if hi_bits == 0 {
            0
        } else {
            (1u32 << hi_bits) - 1
        };
        lo.count_ones() + (hi & hi_mask).count_ones()
    }
}

#[derive(Resource)]
pub struct VoxelMeshData {
    /// Concatenated DAG node data for all chunks in the pool,
    /// followed by pooled foliage bitmaps.
    pub pool_dag: Vec<u32>,
    /// Concatenated average-color data, parallel to pool_dag (DAG portion only).
    pub pool_avg: Vec<u32>,
    /// Per-slot indirection: [world_size, root_offset, depth, pool_offset,
    ///                        foliage_y_min, foliage_y_max, foliage_bitmap_offset,
    ///                        foliage_y_bands, foliage_tile_y_ranges_offset,
    ///                        solid_min_xyz, solid_max_xyz, flags] × total_slots.
    pub indirection: Vec<u32>,
    /// Chunk-coordinate origin of the grid (min corner).
    pub grid_min: [i32; 3],
    /// Grid extent per axis [dim_x, dim_y, dim_z].
    pub grid_dim: [u32; 3],
    /// Horizontal chunk extent in voxels (X and Z).
    pub chunk_size_xz: u32,
    /// Vertical chunk extent in voxels (Y).
    pub chunk_size_y: u32,
    pub material_palette: [[f32; 3]; MATERIAL_PALETTE_SIZE],
    /// Optional raster representation for edited chunks near the camera.
    pub near_mesh: NearVoxelMeshData,
}

/// Write one indirection slot at the given index.
fn write_baked_slot_with_bounds(
    indirection: &mut [u32],
    idx: usize,
    baked: &BakedChunkData,
    pool_offset: u32,
    bitmap_offset: u32,
    tile_y_bands_offset: u32,
    solid_bounds: [u32; 6],
) {
    write_slot_fields(
        indirection,
        idx,
        baked.world_size,
        baked.root_offset,
        baked.depth,
        pool_offset,
        baked.foliage_y_min,
        baked.foliage_y_max,
        bitmap_offset,
        baked.foliage_y_bands,
        tile_y_bands_offset,
        solid_bounds,
    );
}

fn write_slot_fields(
    indirection: &mut [u32],
    idx: usize,
    world_size: u32,
    root_offset: u32,
    depth: u32,
    pool_offset: u32,
    foliage_y_min: u32,
    foliage_y_max: u32,
    bitmap_offset: u32,
    foliage_y_bands: u32,
    tile_y_bands_offset: u32,
    solid_bounds: [u32; 6],
) {
    let base = idx * INDIRECTION_STRIDE;
    indirection[base + SLOT_WORLD_SIZE] = world_size;
    indirection[base + SLOT_ROOT_OFFSET] = root_offset;
    indirection[base + SLOT_DEPTH] = depth;
    indirection[base + SLOT_POOL_OFFSET] = pool_offset;
    indirection[base + SLOT_FOLIAGE_Y_MIN] = foliage_y_min;
    indirection[base + SLOT_FOLIAGE_Y_MAX] = foliage_y_max;
    indirection[base + SLOT_FOLIAGE_BITMAP_OFFSET] = bitmap_offset;
    indirection[base + SLOT_FOLIAGE_Y_BANDS] = foliage_y_bands;
    indirection[base + SLOT_FOLIAGE_TILE_Y_RANGES_OFFSET] = tile_y_bands_offset;
    indirection[base + SLOT_SOLID_MIN_X] = solid_bounds[0];
    indirection[base + SLOT_SOLID_MIN_Y] = solid_bounds[1];
    indirection[base + SLOT_SOLID_MIN_Z] = solid_bounds[2];
    indirection[base + SLOT_SOLID_MAX_X] = solid_bounds[3];
    indirection[base + SLOT_SOLID_MAX_Y] = solid_bounds[4];
    indirection[base + SLOT_SOLID_MAX_Z] = solid_bounds[5];
    indirection[base + SLOT_FLAGS] = 0;
}

#[derive(Clone, Copy)]
struct SolidAabb {
    min: [u32; 3],
    max: [u32; 3],
}

impl SolidAabb {
    fn empty() -> Self {
        Self {
            min: [u32::MAX; 3],
            max: [0; 3],
        }
    }

    fn include(&mut self, min: [u32; 3], max: [u32; 3]) {
        self.min[0] = self.min[0].min(min[0]);
        self.min[1] = self.min[1].min(min[1]);
        self.min[2] = self.min[2].min(min[2]);
        self.max[0] = self.max[0].max(max[0]);
        self.max[1] = self.max[1].max(max[1]);
        self.max[2] = self.max[2].max(max[2]);
    }

    fn into_slot_bounds(self, world_size: u32) -> [u32; 6] {
        if self.min[0] == u32::MAX {
            return [0; 6];
        }
        [
            self.min[0].min(world_size),
            self.min[1].min(world_size),
            self.min[2].min(world_size),
            self.max[0].min(world_size),
            self.max[1].min(world_size),
            self.max[2].min(world_size),
        ]
    }
}

fn solid_aabb_for_baked(baked: &BakedChunkData) -> [u32; 6] {
    if baked.world_size == 0 || baked.depth == 0 {
        return [0; 6];
    }

    let mut aabb = SolidAabb::empty();
    collect_solid_aabb(
        &baked.dag_buffer,
        baked.root_offset as usize,
        [0; 3],
        baked.world_size,
        baked.depth,
        &mut aabb,
    );
    aabb.into_slot_bounds(baked.world_size)
}

fn collect_solid_aabb(
    buffer: &[u32],
    node_offset: usize,
    origin: [u32; 3],
    size: u32,
    remaining_depth: u32,
    aabb: &mut SolidAabb,
) {
    let Some((&mask_lo, rest)) = buffer.get(node_offset).zip(buffer.get(node_offset + 1..)) else {
        return;
    };
    let Some((&mask_hi, rest)) = rest.split_first() else {
        return;
    };
    let Some((&flags, _)) = rest.split_first() else {
        return;
    };

    if (flags & NODE_FLAG_UNIFORM_WATER) != 0 {
        return;
    }

    let child_size = (size / TREE_BRANCH).max(1);
    let is_leaf = (flags & NODE_FLAG_LEAF) != 0 || remaining_depth <= 1;

    for bit in 0..64 {
        if !bit_is_set_64(mask_lo, mask_hi, bit) {
            continue;
        }

        let lx = bit % TREE_BRANCH;
        let ly = (bit / TREE_BRANCH) % TREE_BRANCH;
        let lz = bit / (TREE_BRANCH * TREE_BRANCH);
        let child_min = [
            origin[0] + lx * child_size,
            origin[1] + ly * child_size,
            origin[2] + lz * child_size,
        ];
        let child_max = [
            child_min[0] + child_size,
            child_min[1] + child_size,
            child_min[2] + child_size,
        ];

        if is_leaf {
            let mat = leaf_material_from_buffer(buffer, node_offset, bit);
            if mat != 0 && !is_water_material(mat) {
                aabb.include(child_min, child_max);
            }
            continue;
        }

        let packed_index = popcount_below(mask_lo, mask_hi, bit) as usize;
        let Some(&child_offset) = buffer.get(node_offset + 3 + packed_index) else {
            // Bad tree data should not make the GPU cull a potentially visible chunk.
            aabb.include(child_min, child_max);
            continue;
        };
        collect_solid_aabb(
            buffer,
            child_offset as usize,
            child_min,
            child_size,
            remaining_depth - 1,
            aabb,
        );
    }
}

fn leaf_material_from_buffer(buffer: &[u32], node_offset: usize, bit: u32) -> MaterialId {
    let word_index = bit as usize / 2;
    let half_offset = bit % 2;
    let word = buffer
        .get(node_offset + 3 + word_index)
        .copied()
        .unwrap_or(0);
    ((word >> (half_offset * 16)) & 0xFFFF) as MaterialId
}

/// Compute the foliage bitmap pool offset for a baked chunk.
/// Appends the bitmap to `pool` if needed and returns the offset sentinel.
fn pool_foliage_bitmap(
    pool: &mut Vec<u32>,
    foliage_y_min: u32,
    foliage_y_max: u32,
    foliage_bitmap: &Option<Vec<u32>>,
) -> u32 {
    if foliage_y_min >= foliage_y_max {
        return FOLIAGE_BITMAP_NONE;
    }
    match foliage_bitmap {
        None => FOLIAGE_BITMAP_ALL,
        Some(bitmap) => {
            let offset = pool.len() as u32;
            pool.extend_from_slice(bitmap);
            offset
        }
    }
}

/// Pool the per-tile Y-range data into the shared DAG/foliage pool.
/// Returns the offset into `pool`, or `FOLIAGE_TILE_NONE` when there is no per-tile data.
fn pool_foliage_tile_y_ranges(pool: &mut Vec<u32>, tile_y_bands: &Option<Vec<u32>>) -> u32 {
    match tile_y_bands {
        None => FOLIAGE_TILE_NONE,
        Some(tiles) => {
            let offset = pool.len() as u32;
            pool.extend_from_slice(tiles);
            offset
        }
    }
}

impl VoxelMeshData {
    pub fn material_at(&self, world_pos: [f32; 3]) -> MaterialId {
        let chunk_size_xz = self.chunk_size_xz as f32;
        let chunk_size_y = self.chunk_size_y as f32;
        let chunk_coord = [
            (world_pos[0] / chunk_size_xz).floor() as i32,
            (world_pos[1] / chunk_size_y).floor() as i32,
            (world_pos[2] / chunk_size_xz).floor() as i32,
        ];
        let Some(info) = self.lookup_chunk_info(chunk_coord) else {
            return 0;
        };

        let chunk_min = [
            chunk_coord[0] as f32 * chunk_size_xz,
            chunk_coord[1] as f32 * chunk_size_y,
            chunk_coord[2] as f32 * chunk_size_xz,
        ];
        let world_size = info.world_size as f32;
        if world_size <= 0.0 {
            return 0;
        }

        let mut scaled = [
            ((world_pos[0] - chunk_min[0]) / world_size).clamp(0.0, MAX_CHILD_FRACTION),
            ((world_pos[1] - chunk_min[1]) / world_size).clamp(0.0, MAX_CHILD_FRACTION),
            ((world_pos[2] - chunk_min[2]) / world_size).clamp(0.0, MAX_CHILD_FRACTION),
        ];

        let pool_base = info.pool_offset as usize;
        let mut node_offset = info.root_offset as usize;
        let mut mask_lo = self
            .pool_dag
            .get(pool_base + node_offset)
            .copied()
            .unwrap_or(0);
        let mut mask_hi = self
            .pool_dag
            .get(pool_base + node_offset + 1)
            .copied()
            .unwrap_or(0);
        let mut is_leaf = self
            .pool_dag
            .get(pool_base + node_offset + 2)
            .copied()
            .unwrap_or(0)
            & 1
            != 0;

        for _ in 0..info.depth {
            let cell_x = (scaled[0] * 4.0).floor().clamp(0.0, 3.0) as u32;
            let cell_y = (scaled[1] * 4.0).floor().clamp(0.0, 3.0) as u32;
            let cell_z = (scaled[2] * 4.0).floor().clamp(0.0, 3.0) as u32;
            let cell_index = cell_x + cell_y * 4 + cell_z * 16;

            if is_leaf {
                if !bit_is_set_64(mask_lo, mask_hi, cell_index) {
                    return 0;
                }
                return self.leaf_material(pool_base, node_offset, cell_index);
            }

            if !bit_is_set_64(mask_lo, mask_hi, cell_index) {
                return 0;
            }

            let packed_index = popcount_below(mask_lo, mask_hi, cell_index) as usize;
            // Pool child pointers carry tag bits in the top bits — mask them off.
            node_offset = (self
                .pool_dag
                .get(pool_base + node_offset + 3 + packed_index)
                .copied()
                .unwrap_or(0)
                & POOL_CHILD_OFFSET_MASK) as usize;
            mask_lo = self
                .pool_dag
                .get(pool_base + node_offset)
                .copied()
                .unwrap_or(0);
            mask_hi = self
                .pool_dag
                .get(pool_base + node_offset + 1)
                .copied()
                .unwrap_or(0);
            is_leaf = self
                .pool_dag
                .get(pool_base + node_offset + 2)
                .copied()
                .unwrap_or(0)
                & 1
                != 0;

            scaled = [
                (scaled[0] * 4.0 - cell_x as f32).clamp(0.0, MAX_CHILD_FRACTION),
                (scaled[1] * 4.0 - cell_y as f32).clamp(0.0, MAX_CHILD_FRACTION),
                (scaled[2] * 4.0 - cell_z as f32).clamp(0.0, MAX_CHILD_FRACTION),
            ];
        }

        0
    }

    pub fn is_water_at(&self, world_pos: [f32; 3]) -> bool {
        is_water_material(self.material_at(world_pos))
    }

    pub fn empty() -> Self {
        Self {
            pool_dag: vec![0],
            pool_avg: vec![0],
            indirection: vec![0; INDIRECTION_STRIDE],
            grid_min: [0; 3],
            grid_dim: [1, 1, 1],
            chunk_size_xz: 1,
            chunk_size_y: 1,
            material_palette: [[0.0; 3]; MATERIAL_PALETTE_SIZE],
            near_mesh: NearVoxelMeshData::default(),
        }
    }

    /// Wrap a single baked chunk as a 1×1×1 grid (backward compat for asset loading).
    pub fn from_single_chunk(
        baked: BakedChunkData,
        chunk_size_xz: u32,
        chunk_size_y: u32,
        material_palette: [[f32; 3]; MATERIAL_PALETTE_SIZE],
    ) -> Self {
        let solid_bounds = solid_aabb_for_baked(&baked);
        let mut pool_dag = baked.dag_buffer;
        tag_pool_child_pointers(&mut pool_dag, baked.root_offset);
        let bitmap_offset = pool_foliage_bitmap(
            &mut pool_dag,
            baked.foliage_y_min,
            baked.foliage_y_max,
            &baked.foliage_bitmap,
        );
        let tile_offset = pool_foliage_tile_y_ranges(&mut pool_dag, &baked.foliage_tile_y_ranges);
        let mut indirection = vec![0u32; INDIRECTION_STRIDE];
        write_slot_fields(
            &mut indirection,
            0,
            baked.world_size,
            baked.root_offset,
            baked.depth,
            0,
            baked.foliage_y_min,
            baked.foliage_y_max,
            bitmap_offset,
            baked.foliage_y_bands,
            tile_offset,
            solid_bounds,
        );
        Self {
            pool_dag,
            pool_avg: baked.avg_color_buffer,
            indirection,
            grid_min: [0, 0, 0],
            grid_dim: [1, 1, 1],
            chunk_size_xz,
            chunk_size_y,
            material_palette,
            near_mesh: NearVoxelMeshData::default(),
        }
    }

    /// Build a grid_dim_xz × 1 × grid_dim_xz flat world from a single canonical chunk.
    /// All slots in the Y=0 layer point to the same DAG data at pool_offset=0.
    pub fn from_flat_world(
        canonical: &BakedChunkData,
        grid_dim_xz: u32,
        chunk_size_xz: u32,
        chunk_size_y: u32,
        material_palette: [[f32; 3]; MATERIAL_PALETTE_SIZE],
    ) -> Self {
        let half = (grid_dim_xz / 2) as i32;
        let grid_dim = [grid_dim_xz, 1u32, grid_dim_xz];
        let total_slots = (grid_dim[0] * grid_dim[1] * grid_dim[2]) as usize;

        let mut pool_dag = canonical.dag_buffer.clone();
        tag_pool_child_pointers(&mut pool_dag, canonical.root_offset);
        // Canonical chunk: pool bitmap once, all slots share it.
        let bitmap_offset = pool_foliage_bitmap(
            &mut pool_dag,
            canonical.foliage_y_min,
            canonical.foliage_y_max,
            &canonical.foliage_bitmap,
        );
        let tile_offset =
            pool_foliage_tile_y_ranges(&mut pool_dag, &canonical.foliage_tile_y_ranges);
        let canonical_solid_bounds = solid_aabb_for_baked(canonical);

        let mut indirection = vec![0u32; total_slots * INDIRECTION_STRIDE];
        for z in 0..grid_dim_xz {
            for x in 0..grid_dim_xz {
                let idx = (x + z * grid_dim_xz) as usize;
                write_baked_slot_with_bounds(
                    &mut indirection,
                    idx,
                    canonical,
                    0,
                    bitmap_offset,
                    tile_offset,
                    canonical_solid_bounds,
                );
            }
        }

        Self {
            pool_dag,
            pool_avg: canonical.avg_color_buffer.clone(),
            indirection,
            grid_min: [-half, 0, -half],
            grid_dim,
            chunk_size_xz,
            chunk_size_y,
            material_palette,
            near_mesh: NearVoxelMeshData::default(),
        }
    }

    /// Rebuild the mesh with multiple edited chunks replacing their canonical slots.
    /// Keys in `edited` are chunk coordinates (e.g. [0, 0, 0]).
    pub fn with_edited_chunks(
        canonical: &BakedChunkData,
        edited: &HashMap<[i32; 3], BakedChunkData>,
        grid_dim_xz: u32,
        chunk_size_xz: u32,
        chunk_size_y: u32,
        material_palette: [[f32; 3]; MATERIAL_PALETTE_SIZE],
    ) -> Self {
        let half = (grid_dim_xz / 2) as i32;
        let grid_dim = [grid_dim_xz, 1u32, grid_dim_xz];
        let total_slots = (grid_dim[0] * grid_dim[1] * grid_dim[2]) as usize;

        let total_dag_words = canonical.dag_buffer.len()
            + edited
                .values()
                .map(|baked| baked.dag_buffer.len())
                .sum::<usize>();
        let total_avg_words = canonical.avg_color_buffer.len()
            + edited
                .values()
                .map(|baked| baked.avg_color_buffer.len())
                .sum::<usize>();

        // Pool: canonical DAG at offset 0, edited DAGs appended after.
        let mut pool_dag = Vec::with_capacity(total_dag_words);
        pool_dag.extend_from_slice(&canonical.dag_buffer);
        tag_pool_child_pointers(&mut pool_dag, canonical.root_offset);

        let mut pool_avg = Vec::with_capacity(total_avg_words);
        pool_avg.extend_from_slice(&canonical.avg_color_buffer);

        let mut edited_offsets: HashMap<[i32; 3], u32> = HashMap::with_capacity(edited.len());
        let mut edited_solid_bounds: HashMap<[i32; 3], [u32; 6]> =
            HashMap::with_capacity(edited.len());
        for (coord, baked) in edited {
            let offset = pool_dag.len() as u32;
            pool_dag.extend_from_slice(&baked.dag_buffer);
            tag_pool_child_pointers(&mut pool_dag[offset as usize..], baked.root_offset);
            pool_avg.extend_from_slice(&baked.avg_color_buffer);
            edited_offsets.insert(*coord, offset);
            edited_solid_bounds.insert(*coord, solid_aabb_for_baked(baked));
        }

        // Pool foliage bitmaps and tile Y-range data after all DAG data.
        let canonical_bmp = pool_foliage_bitmap(
            &mut pool_dag,
            canonical.foliage_y_min,
            canonical.foliage_y_max,
            &canonical.foliage_bitmap,
        );
        let canonical_tile =
            pool_foliage_tile_y_ranges(&mut pool_dag, &canonical.foliage_tile_y_ranges);
        let canonical_solid_bounds = solid_aabb_for_baked(canonical);
        let mut edited_bmp_offsets: HashMap<[i32; 3], (u32, u32)> =
            HashMap::with_capacity(edited.len());
        for (coord, baked) in edited {
            let bmp = pool_foliage_bitmap(
                &mut pool_dag,
                baked.foliage_y_min,
                baked.foliage_y_max,
                &baked.foliage_bitmap,
            );
            let tile = pool_foliage_tile_y_ranges(&mut pool_dag, &baked.foliage_tile_y_ranges);
            edited_bmp_offsets.insert(*coord, (bmp, tile));
        }

        // Build indirection: canonical by default, override edited slots
        let grid_min = [-half, 0i32, -half];
        let mut indirection = vec![0u32; total_slots * INDIRECTION_STRIDE];
        for z in 0..grid_dim_xz {
            for x in 0..grid_dim_xz {
                let idx = (x + z * grid_dim_xz) as usize;
                let chunk_coord = [x as i32 + grid_min[0], 0i32, z as i32 + grid_min[2]];

                if let Some(&pool_offset) = edited_offsets.get(&chunk_coord) {
                    let baked = &edited[&chunk_coord];
                    let (bmp, tile) = edited_bmp_offsets[&chunk_coord];
                    write_baked_slot_with_bounds(
                        &mut indirection,
                        idx,
                        baked,
                        pool_offset,
                        bmp,
                        tile,
                        edited_solid_bounds[&chunk_coord],
                    );
                } else {
                    write_baked_slot_with_bounds(
                        &mut indirection,
                        idx,
                        canonical,
                        0,
                        canonical_bmp,
                        canonical_tile,
                        canonical_solid_bounds,
                    );
                }
            }
        }

        Self {
            pool_dag,
            pool_avg,
            indirection,
            grid_min,
            grid_dim,
            chunk_size_xz,
            chunk_size_y,
            material_palette,
            near_mesh: NearVoxelMeshData::default(),
        }
    }

    /// Attach a complete near-field representation and flag its chunks in the
    /// GPU indirection table. A chunk is skipped by primary voxel traversal only
    /// when it appears in `near_mesh.chunks`.
    pub fn set_near_mesh(&mut self, near_mesh: NearVoxelMeshData) {
        for slot in self.indirection.chunks_exact_mut(INDIRECTION_STRIDE) {
            slot[SLOT_FLAGS] &= !SLOT_FLAG_NEAR_MESH;
        }

        for chunk in &near_mesh.chunks {
            let coord = chunk.coord;
            let local_x = coord[0] - self.grid_min[0];
            let local_y = coord[1] - self.grid_min[1];
            let local_z = coord[2] - self.grid_min[2];
            if local_x < 0
                || local_y < 0
                || local_z < 0
                || local_x >= self.grid_dim[0] as i32
                || local_y >= self.grid_dim[1] as i32
                || local_z >= self.grid_dim[2] as i32
            {
                continue;
            }
            let slot_index = local_x as usize
                + local_y as usize * self.grid_dim[0] as usize
                + local_z as usize * self.grid_dim[0] as usize * self.grid_dim[1] as usize;
            self.indirection[slot_index * INDIRECTION_STRIDE + SLOT_FLAGS] |= SLOT_FLAG_NEAR_MESH;
        }

        for &coord in &near_mesh.canonical_chunks {
            let local_x = coord[0] - self.grid_min[0];
            let local_y = coord[1] - self.grid_min[1];
            let local_z = coord[2] - self.grid_min[2];
            if local_x < 0
                || local_y < 0
                || local_z < 0
                || local_x >= self.grid_dim[0] as i32
                || local_y >= self.grid_dim[1] as i32
                || local_z >= self.grid_dim[2] as i32
            {
                continue;
            }
            let slot_index = local_x as usize
                + local_y as usize * self.grid_dim[0] as usize
                + local_z as usize * self.grid_dim[0] as usize * self.grid_dim[1] as usize;
            self.indirection[slot_index * INDIRECTION_STRIDE + SLOT_FLAGS] |= SLOT_FLAG_NEAR_MESH;
        }

        self.near_mesh = near_mesh;
    }

    fn lookup_chunk_info(&self, chunk_coord: [i32; 3]) -> Option<SlotTreeInfo> {
        let local_x = chunk_coord[0] - self.grid_min[0];
        let local_y = chunk_coord[1] - self.grid_min[1];
        let local_z = chunk_coord[2] - self.grid_min[2];
        if local_x < 0
            || local_y < 0
            || local_z < 0
            || local_x >= self.grid_dim[0] as i32
            || local_y >= self.grid_dim[1] as i32
            || local_z >= self.grid_dim[2] as i32
        {
            return None;
        }

        let slot_index = local_x as usize
            + local_y as usize * self.grid_dim[0] as usize
            + local_z as usize * self.grid_dim[0] as usize * self.grid_dim[1] as usize;
        let base = slot_index * INDIRECTION_STRIDE;
        let world_size = *self.indirection.get(base + SLOT_WORLD_SIZE)?;
        if world_size == 0 {
            return None;
        }

        Some(SlotTreeInfo {
            world_size,
            root_offset: *self.indirection.get(base + SLOT_ROOT_OFFSET)?,
            depth: *self.indirection.get(base + SLOT_DEPTH)?,
            pool_offset: *self.indirection.get(base + SLOT_POOL_OFFSET)?,
        })
    }

    fn leaf_material(&self, pool_base: usize, node_offset: usize, bit: u32) -> MaterialId {
        let word_index = bit as usize / 2;
        let half_offset = bit % 2;
        let word = self
            .pool_dag
            .get(pool_base + node_offset + 3 + word_index)
            .copied()
            .unwrap_or(0);
        ((word >> (half_offset * 16)) & 0xFFFF) as MaterialId
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BakedChunkData;

    /// Two-level tree (world_size 16, depth 2): a leaf brick with material 7
    /// at voxel (0,0,0), referenced by the root's first child slot.
    fn tiny_baked_chunk() -> BakedChunkData {
        let mut buf = Vec::new();
        // Leaf node at offset 0: header + 32 material words.
        buf.push(1); // mask_lo: voxel bit 0 set
        buf.push(0); // mask_hi
        buf.push(NODE_FLAG_LEAF);
        let mut materials = [0u32; 32];
        materials[0] = 7; // low halfword of word 0 = material at bit 0
        buf.extend_from_slice(&materials);
        // Root inner node at offset 35: child cell 0 -> leaf.
        let root = buf.len() as u32;
        buf.push(1); // mask_lo: child bit 0 set
        buf.push(0); // mask_hi
        buf.push(0); // flags: inner
        buf.push(0); // child pointer -> leaf at offset 0

        BakedChunkData {
            avg_color_buffer: vec![0; buf.len()],
            dag_buffer: buf,
            root_offset: root,
            world_size: 16,
            depth: 2,
            foliage_y_min: 0,
            foliage_y_max: 0,
            foliage_bitmap: None,
            foliage_y_bands: 0,
            foliage_tile_y_ranges: None,
        }
    }

    #[test]
    fn pool_child_pointers_are_tagged_and_masked() {
        let baked = tiny_baked_chunk();
        let root = baked.root_offset as usize;
        let mesh = VoxelMeshData::from_single_chunk(baked, 16, 16, [[0.0; 3]; 1024]);

        // The root's child pointer should carry the leaf tag bit.
        let child_ptr = mesh.pool_dag[root + 3];
        assert_eq!(child_ptr & POOL_CHILD_OFFSET_MASK, 0);
        assert_ne!(child_ptr & POOL_CHILD_FLAG_LEAF, 0);
        assert_eq!(child_ptr & POOL_CHILD_FLAG_UNIFORM_WATER, 0);

        // material_at must mask the tag bits when following the pointer.
        assert_eq!(mesh.material_at([0.5, 0.5, 0.5]), 7);
        assert_eq!(mesh.material_at([5.0, 5.0, 5.0]), 0);
    }

    #[test]
    fn tagging_is_idempotent() {
        let baked = tiny_baked_chunk();
        let mut once = baked.dag_buffer.clone();
        tag_pool_child_pointers(&mut once, baked.root_offset);
        let mut twice = once.clone();
        tag_pool_child_pointers(&mut twice, baked.root_offset);
        assert_eq!(once, twice);
    }
}
