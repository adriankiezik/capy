#[cfg(feature = "cpu-bench")]
mod bench;

mod config;
mod error;
mod storage;
mod transaction;

pub use capy_engine_protocol::world::{
    Material, MaterialId, Owner, OwnerId, StructureId, VOXEL_SIZE, Voxel, VoxelBounds, VoxelCoord,
    voxel_bounds,
};
pub use config::WorldSettings;
pub use error::{Result, WorldError};
pub use storage::WorldRead;
pub(crate) use storage::{
    LEAF_EDGE, LEAF_VOXELS, Leaf, LeafCoord, LeafSources, address, coordinate,
};
pub use transaction::Transaction;
pub(crate) use transaction::prepare;

#[cfg(test)]
pub(crate) mod tests;
