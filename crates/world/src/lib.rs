mod bake;
mod dag;
mod error;
mod sparse64tree;
mod terrain;
mod voxel_grid;

pub use error::WorldError;
pub use terrain::CHUNK_SIZE;

pub fn generate_baked_terrain(seed: u32) -> error::Result<capy_core::BakedChunkData> {
    let (grid, heights) = terrain::generate_terrain_grid(seed)?;
    bake::bake_chunk(&grid, Some(&heights))
}
