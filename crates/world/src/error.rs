use thiserror::Error;

#[derive(Debug, Error)]
pub enum WorldError {
    #[error("invalid voxel grid dimensions: expected {expected} voxels, got {actual}")]
    InvalidGridDimensions { expected: usize, actual: usize },
}

pub type Result<T> = std::result::Result<T, WorldError>;
