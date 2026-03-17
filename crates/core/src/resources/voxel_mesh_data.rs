use std::collections::HashMap;

use bevy_ecs::resource::Resource;

use crate::{BakedChunkData, MATERIAL_PALETTE_SIZE};

#[derive(Resource)]
pub struct VoxelMeshData {
    /// Concatenated DAG node data for all chunks in the pool.
    pub pool_dag: Vec<u32>,
    /// Concatenated average-color data, parallel to pool_dag.
    pub pool_avg: Vec<u32>,
    /// Per-slot indirection: [world_size, root_offset, depth, pool_offset] × total_slots.
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
}

impl VoxelMeshData {
    pub fn empty() -> Self {
        Self {
            pool_dag: vec![0],
            pool_avg: vec![0],
            indirection: vec![0; 4],
            grid_min: [0; 3],
            grid_dim: [1, 1, 1],
            chunk_size_xz: 1,
            chunk_size_y: 1,
            material_palette: [[0.0; 3]; MATERIAL_PALETTE_SIZE],
        }
    }

    /// Wrap a single baked chunk as a 1×1×1 grid (backward compat for asset loading).
    pub fn from_single_chunk(
        baked: BakedChunkData,
        chunk_size_xz: u32,
        chunk_size_y: u32,
        material_palette: [[f32; 3]; MATERIAL_PALETTE_SIZE],
    ) -> Self {
        let indirection = vec![baked.world_size, baked.root_offset, baked.depth, 0];
        Self {
            pool_dag: baked.dag_buffer,
            pool_avg: baked.avg_color_buffer,
            indirection,
            grid_min: [0, 0, 0],
            grid_dim: [1, 1, 1],
            chunk_size_xz,
            chunk_size_y,
            material_palette,
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

        let mut indirection = vec![0u32; total_slots * 4];
        for z in 0..grid_dim_xz {
            for x in 0..grid_dim_xz {
                let idx = (x + z * grid_dim_xz) as usize;
                indirection[idx * 4] = canonical.world_size;
                indirection[idx * 4 + 1] = canonical.root_offset;
                indirection[idx * 4 + 2] = canonical.depth;
                indirection[idx * 4 + 3] = 0; // all share same pool data
            }
        }

        Self {
            pool_dag: canonical.dag_buffer.clone(),
            pool_avg: canonical.avg_color_buffer.clone(),
            indirection,
            grid_min: [-half, 0, -half],
            grid_dim,
            chunk_size_xz,
            chunk_size_y,
            material_palette,
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

        // Build indirection: canonical by default, override edited slots
        let grid_min = [-half, 0i32, -half];
        let mut indirection = vec![0u32; total_slots * 4];
        for z in 0..grid_dim_xz {
            for x in 0..grid_dim_xz {
                let idx = (x + z * grid_dim_xz) as usize;
                let chunk_coord = [x as i32 + grid_min[0], 0i32, z as i32 + grid_min[2]];

                if let Some(&pool_offset) = edited_offsets.get(&chunk_coord) {
                    let baked = &edited[&chunk_coord];
                    indirection[idx * 4] = baked.world_size;
                    indirection[idx * 4 + 1] = baked.root_offset;
                    indirection[idx * 4 + 2] = baked.depth;
                    indirection[idx * 4 + 3] = pool_offset;
                } else {
                    indirection[idx * 4] = canonical.world_size;
                    indirection[idx * 4 + 1] = canonical.root_offset;
                    indirection[idx * 4 + 2] = canonical.depth;
                    indirection[idx * 4 + 3] = 0;
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
        }
    }
}
