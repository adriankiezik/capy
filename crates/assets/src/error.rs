use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum AssetError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("invalid file magic: expected {expected:?}, got {actual:?}")]
    InvalidMagic { expected: [u8; 4], actual: [u8; 4] },

    #[error("unsupported format version: {0}")]
    UnsupportedVersion(u16),

    #[error("unsupported compression: {0}")]
    UnsupportedCompression(u8),

    #[error("hash mismatch for region ({rx}, {ry}, {rz})")]
    HashMismatch { rx: i32, ry: i32, rz: i32 },

    #[error("decompression failed: {0}")]
    DecompressFailed(String),

    #[error("manifest not found: {0}")]
    ManifestNotFound(PathBuf),

    #[error("region file not found: {0}")]
    RegionNotFound(PathBuf),

    #[error("corrupt region file: {reason}")]
    CorruptRegion { reason: String },

    #[error("region has too many chunks for format: {count} (max {max})")]
    TooManyChunksInRegion { count: usize, max: usize },
}

pub type Result<T> = std::result::Result<T, AssetError>;
