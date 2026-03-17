use thiserror::Error;

#[derive(Debug, Error)]
pub enum WorldError {
    #[error("invalid voxel grid dimensions: expected {expected} voxels, got {actual}")]
    InvalidGridDimensions { expected: usize, actual: usize },

    #[error("background bake thread disconnected without producing a result")]
    BakeFailed,
}

pub type Result<T> = std::result::Result<T, WorldError>;
