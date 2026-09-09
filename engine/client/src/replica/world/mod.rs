mod storage;

pub use capy_engine_protocol::world::{
    MaterialId, OwnerId, VOXEL_SIZE, Voxel, VoxelBounds, VoxelCoord,
};
#[cfg(test)]
pub use capy_engine_protocol::world::{Owner, StructureId};
#[cfg(test)]
pub(crate) use storage::coordinate;
pub(crate) use storage::{LEAF_EDGE, Leaf, WorldRead, address};

#[cfg(any(test, feature = "cpu-bench"))]
pub use capy_engine_protocol::world::Material;
