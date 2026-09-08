use std::{io, path::PathBuf, sync::Arc};
use thiserror::Error;

pub type Result<T> = std::result::Result<T, AssetError>;

#[derive(Clone, Debug, Error)]
#[non_exhaustive]
pub enum AssetError {
    #[error("Asset worker count must be nonzero")]
    InvalidWorkerCount,
    #[error("Invalid asset path: {0}")]
    InvalidPath(PathBuf),
    #[error("Reading asset {path}: {source}")]
    Io {
        path: PathBuf,
        source: Arc<io::Error>,
    },
    #[error("Decoding asset {path}: {message}")]
    Decode { path: PathBuf, message: String },
    #[error("Asset worker unavailable: {0}")]
    WorkerUnavailable(String),
    #[error("Asset loader panicked while loading {0}")]
    LoaderPanicked(PathBuf),
}
