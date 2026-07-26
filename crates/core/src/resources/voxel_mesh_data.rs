use std::collections::HashMap;

use bevy_ecs::resource::Resource;

use crate::{BakedChunkData, MATERIAL_PALETTE_SIZE, MaterialId, is_water_material};

/// Number of u32 words per slot in the indirection table.
const INDIRECTION_STRIDE: usize = 10;
const SLOT_FLAG_NEAR_MESH: u32 = 1;

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
    ///                        flags] × total_slots.
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
fn write_slot(
    indirection: &mut [u32],
    idx: usize,
    baked: &BakedChunkData,
    pool_offset: u32,
    bitmap_offset: u32,
    tile_y_bands_offset: u32,
) {
    let base = idx * INDIRECTION_STRIDE;
    indirection[base] = baked.world_size;
    indirection[base + 1] = baked.root_offset;
    indirection[base + 2] = baked.depth;
    indirection[base + 3] = pool_offset;
    indirection[base + 4] = baked.foliage_y_min;
    indirection[base + 5] = baked.foliage_y_max;
    indirection[base + 6] = bitmap_offset;
    indirection[base + 7] = baked.foliage_y_bands;
    indirection[base + 8] = tile_y_bands_offset;
    indirection[base + 9] = 0;
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
            node_offset = self
                .pool_dag
                .get(pool_base + node_offset + 3 + packed_index)
                .copied()
                .unwrap_or(0) as usize;
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
        let mut pool_dag = baked.dag_buffer;
        let bitmap_offset = pool_foliage_bitmap(
            &mut pool_dag,
            baked.foliage_y_min,
            baked.foliage_y_max,
            &baked.foliage_bitmap,
        );
        let tile_offset = pool_foliage_tile_y_ranges(&mut pool_dag, &baked.foliage_tile_y_ranges);
        let mut indirection = vec![0u32; INDIRECTION_STRIDE];
        let base = 0;
        indirection[base] = baked.world_size;
        indirection[base + 1] = baked.root_offset;
        indirection[base + 2] = baked.depth;
        indirection[base + 3] = 0;
        indirection[base + 4] = baked.foliage_y_min;
        indirection[base + 5] = baked.foliage_y_max;
        indirection[base + 6] = bitmap_offset;
        indirection[base + 7] = baked.foliage_y_bands;
        indirection[base + 8] = tile_offset;
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
        // Canonical chunk: pool bitmap once, all slots share it.
        let bitmap_offset = pool_foliage_bitmap(
            &mut pool_dag,
            canonical.foliage_y_min,
            canonical.foliage_y_max,
            &canonical.foliage_bitmap,
        );
        let tile_offset =
            pool_foliage_tile_y_ranges(&mut pool_dag, &canonical.foliage_tile_y_ranges);

        let mut indirection = vec![0u32; total_slots * INDIRECTION_STRIDE];
        for z in 0..grid_dim_xz {
            for x in 0..grid_dim_xz {
                let idx = (x + z * grid_dim_xz) as usize;
                write_slot(
                    &mut indirection,
                    idx,
                    canonical,
                    0,
                    bitmap_offset,
                    tile_offset,
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

        let mut pool_avg = Vec::with_capacity(total_avg_words);
        pool_avg.extend_from_slice(&canonical.avg_color_buffer);

        let mut edited_offsets: HashMap<[i32; 3], u32> = HashMap::with_capacity(edited.len());
        for (coord, baked) in edited {
            let offset = pool_dag.len() as u32;
            pool_dag.extend_from_slice(&baked.dag_buffer);
            pool_avg.extend_from_slice(&baked.avg_color_buffer);
            edited_offsets.insert(*coord, offset);
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
                    write_slot(&mut indirection, idx, baked, pool_offset, bmp, tile);
                } else {
                    write_slot(
                        &mut indirection,
                        idx,
                        canonical,
                        0,
                        canonical_bmp,
                        canonical_tile,
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
            slot[9] &= !SLOT_FLAG_NEAR_MESH;
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
            self.indirection[slot_index * INDIRECTION_STRIDE + 9] |= SLOT_FLAG_NEAR_MESH;
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
            self.indirection[slot_index * INDIRECTION_STRIDE + 9] |= SLOT_FLAG_NEAR_MESH;
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
        let world_size = *self.indirection.get(base)?;
        if world_size == 0 {
            return None;
        }

        Some(SlotTreeInfo {
            world_size,
            root_offset: *self.indirection.get(base + 1)?,
            depth: *self.indirection.get(base + 2)?,
            pool_offset: *self.indirection.get(base + 3)?,
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
