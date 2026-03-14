use capy_core::VoxelMeshData;
use noise::{NoiseFn, Perlin};

use crate::dag::reduce_to_dag;
use crate::error::Result;
use crate::material::MATERIAL_COLORS;
use crate::sparse64tree::{build_and_serialize_tree_with_heights, tree_to_gpu_data};
use crate::voxel_grid::VoxelGrid;

pub const CHUNK_SIZE: u32 = 256;

pub fn generate_terrain(seed: u32) -> Result<VoxelMeshData> {
    let perlin = Perlin::new(seed);

    let cs = CHUNK_SIZE as usize;
    let global_height = CHUNK_SIZE as f64;
    let height_scale = global_height * 0.45;
    let base_height = global_height * 0.25;
    let freq = 6.0 / 1024.0;
    let freq2 = freq * 4.0;

    let mut col_heights = vec![0u16; cs * cs];
    let total = cs * cs * cs;
    let mut grid = vec![0u8; total];

    for lz in 0..cs {
        let wz = lz as f64;
        let row_base = lz * cs;
        for lx in 0..cs {
            let wx = lx as f64;

            let coarse = perlin.get([wx * freq, wz * freq]);
            let fine = perlin.get([wx * freq2, wz * freq2]);
            let h = base_height + (coarse * 0.7 + fine * 0.3) * height_scale;
            let fill = h.clamp(0.0, CHUNK_SIZE as f64) as usize;
            col_heights[row_base + lx] = fill as u16;

            if fill > 0 {
                let base_idx = lx + lz * cs * cs;
                for ly in 0..fill.min(cs) {
                    grid[base_idx + ly * cs] = 1;
                }
            }
        }
    }

    let voxel_grid = VoxelGrid::new(CHUNK_SIZE, CHUNK_SIZE, CHUNK_SIZE, grid)?;
    let flat = build_and_serialize_tree_with_heights(&voxel_grid, Some(&col_heights));
    let dag_flat = reduce_to_dag(&flat);
    let (tree_info, dag_buffer, avg_color_buffer) = tree_to_gpu_data(&dag_flat);

    Ok(VoxelMeshData {
        dag_buffer,
        avg_color_buffer,
        world_size: tree_info.world_size,
        root_offset: tree_info.root_offset,
        depth: tree_info.depth,
        chunk_size: CHUNK_SIZE,
        material_palette: MATERIAL_COLORS,
    })
}
