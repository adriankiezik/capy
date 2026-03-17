use capy_core::{BakedChunkData, MaterialId};

use crate::bake;
use crate::error::Result;
use crate::voxel_grid::VoxelGrid;

/// Horizontal chunk extent (X and Z axes).
pub const CHUNK_XZ: u32 = 256;

/// Vertical chunk extent (Y axis). Power of 4 for optimal DAG tree alignment.
pub const CHUNK_Y: u32 = 1024;

/// Default solid-fill height for unedited flat terrain.
pub const FLAT_FILL_HEIGHT: u32 = 128;

/// Material ID used for the flat-fill solid layer (0 = air).
pub const FLAT_FILL_MATERIAL: MaterialId = 1;

/// Generate a flat-world voxel grid: solid from y=0..FLAT_FILL_HEIGHT, air above.
pub fn generate_flat_grid() -> Result<(VoxelGrid, Vec<u16>)> {
    let xs = CHUNK_XZ as usize;
    let ys = CHUNK_Y as usize;
    let zs = CHUNK_XZ as usize;
    let fill = FLAT_FILL_HEIGHT as usize;

    let total = xs * ys * zs;
    let mut data = vec![0 as MaterialId; total];

    // Fill y=0..fill with contiguous memset per z-slice (256 × 32KB fills vs 8.4M scattered writes)
    for z in 0..zs {
        let slice_start = z * xs * ys;
        let fill_end = slice_start + fill * xs;
        data[slice_start..fill_end].fill(FLAT_FILL_MATERIAL);
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
