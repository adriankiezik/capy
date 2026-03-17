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

    #[error("invalid prefab resolution: {0}")]
    InvalidPrefabResolution(u32),

    #[error("FBX import failed for {path}: {reason}")]
    FbxImportFailed { path: PathBuf, reason: String },

    #[error("FBX file contains no mesh geometry: {0}")]
    NoMeshGeometry(PathBuf),

    #[error("voxelized prefab is empty: {0}")]
    EmptyPrefab(PathBuf),

    #[error("prefab voxel grid is too large: {0} voxels")]
    PrefabTooLarge(u64),

    #[error("invalid prefab cache {path}: {reason}")]
    InvalidVoxelCache { path: PathBuf, reason: String },

    #[error("failed to write prefab cache {path}: {reason}")]
    PrefabCacheWriteFailed { path: PathBuf, reason: String },
}

pub type Result<T> = std::result::Result<T, AssetError>;
