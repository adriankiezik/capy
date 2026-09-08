#[cfg(feature = "cpu-bench")]
mod bench;

mod config;
mod error;
mod storage;
mod transaction;
mod types;

pub use config::WorldSettings;
pub use error::{Result, WorldError};
pub use storage::WorldRead;
pub(crate) use storage::{LEAF_EDGE, LEAF_VOXELS, Leaf, LeafCoord, address, coordinate};
pub use transaction::Transaction;
pub(crate) use transaction::prepare;
pub use types::{
    Material, MaterialId, Owner, OwnerId, StructureId, VOXEL_SIZE, Voxel, VoxelBounds, VoxelCoord,
    voxel_bounds,
};

#[cfg(test)]
pub(crate) mod tests;
