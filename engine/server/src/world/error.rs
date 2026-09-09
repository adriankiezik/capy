use crate::world::VoxelCoord;

pub type Result<T> = std::result::Result<T, WorldError>;

#[derive(Clone, Debug, thiserror::Error)]
pub enum WorldError {
    #[error("invalid world configuration or voxel value")]
    Invalid,
    #[error("voxel {0} is outside resolved world residency")]
    Unavailable(VoxelCoord),
    #[error("occupied-voxel conflict at {0}")]
    Conflict(VoxelCoord),
    #[error("voxel {0} is edited more than once")]
    Duplicate(VoxelCoord),
    #[error("transaction belongs to a stale or different snapshot")]
    Stale,
    #[error("world work or capacity limit exceeded: {0}")]
    Limit(&'static str),
}
