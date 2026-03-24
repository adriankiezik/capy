mod bake;
mod dag;
mod error;
mod sparse64tree;
pub mod stress;
mod terrain;
mod tree_patch;
mod voxel_grid;

pub use bake::{bake_chunk, bake_chunk_fast, compact_baked_chunk, recompute_foliage_bitmap};
pub use error::WorldError;
pub use sparse64tree::ChunkOccupancy;
pub use stress::{StressPattern, generate_stress_baked, generate_stress_world};
pub use terrain::{
    CHUNK_XZ, CHUNK_Y, FLAT_FILL_HEIGHT, FLAT_FILL_MATERIAL, WATER_FILL_HEIGHT, WATER_MATERIAL,
    generate_flat_baked, generate_flat_grid,
};
pub use tree_patch::{
    LeafBrickEdit, VoxelEdit, extract_leaf_bricks, patch_baked_chunk, patch_baked_chunk_bricks,
    patch_baked_chunk_bricks_owned,
};
pub use voxel_grid::VoxelGrid;
