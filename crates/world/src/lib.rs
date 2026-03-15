mod bake;
mod dag;
mod error;
mod sparse64tree;
mod terrain;
mod voxel_grid;

pub use error::WorldError;
pub use terrain::{CHUNK_SIZE, PerlinTerrain, TerrainGenerator};

pub fn generate_baked_terrain(seed: u32) -> error::Result<capy_core::BakedChunkData> {
    PerlinTerrain::default().generate(seed)
}
