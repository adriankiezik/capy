use capy_core::{BakedChunkData, FOLIAGE_BIT, MaterialId, WATER_BIT};

use crate::bake;
use crate::error::Result;
use crate::voxel_grid::VoxelGrid;

/// Horizontal chunk extent (X and Z axes).
pub const CHUNK_XZ: u32 = 256;

/// Vertical chunk extent (Y axis). Power of 4 for optimal DAG tree alignment.
pub const CHUNK_Y: u32 = 1024;

/// Default solid-fill height for unedited flat terrain.
pub const FLAT_FILL_HEIGHT: u32 = 128;

/// Height up to which water voxels are placed (y = FLAT_FILL_HEIGHT..WATER_FILL_HEIGHT).
pub const WATER_FILL_HEIGHT: u32 = 132;

/// Material ID for the surface grass layer (top voxel only).
/// Includes the foliage bit so grass blades grow on the surface.
pub const GRASS_MATERIAL: MaterialId = 1 | FOLIAGE_BIT;

/// Material ID for underground dirt (below the grass surface).
pub const DIRT_MATERIAL: MaterialId = 3;

/// Material ID for water voxels (blue palette entry 8 + water flag).
/// When water rendering is disabled, these render as solid blue blocks.
pub const WATER_MATERIAL: MaterialId = 8 | WATER_BIT;

/// Generate a flat-world voxel grid: solid from y=0..FLAT_FILL_HEIGHT, air above.
pub fn generate_flat_grid() -> Result<(VoxelGrid, Vec<u16>)> {
    let xs = CHUNK_XZ as usize;
    let ys = CHUNK_Y as usize;
    let zs = CHUNK_XZ as usize;
    let fill = FLAT_FILL_HEIGHT as usize;

    let total = xs * ys * zs;
    let mut data = vec![0 as MaterialId; total];

    // Fill per z-slice: dirt from y=0..fill-1, grass at y=fill-1 (top surface only)
    for z in 0..zs {
        let slice_start = z * xs * ys;
        let dirt_end = slice_start + (fill - 1) * xs;
        let grass_end = dirt_end + xs;
        data[slice_start..dirt_end].fill(DIRT_MATERIAL);
        data[dirt_end..grass_end].fill(GRASS_MATERIAL);
    }

    let col_heights = vec![FLAT_FILL_HEIGHT as u16; xs * zs];
    let grid = VoxelGrid::new(CHUNK_XZ, CHUNK_Y, CHUNK_XZ, data)?;
    Ok((grid, col_heights))
}

/// Generate a flat-world chunk, fully baked and ready for rendering.
pub fn generate_flat_baked() -> Result<BakedChunkData> {
    let (grid, col_heights) = generate_flat_grid()?;
    bake::bake_chunk(&grid, Some(&col_heights))
}
