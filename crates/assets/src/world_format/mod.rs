mod binary_io;
mod codec;
mod file_system;
mod hash;
mod manifest;
mod region_io;
mod types;
mod world_loader;

pub const DEFAULT_WORLD_DIR: &str = "assets/worlds/default";

/// Resolve the world directory, preferring `<exe_dir>/worlds/default` for
/// release builds, falling back to `assets/worlds/default` (dev layout).
pub fn resolve_world_dir() -> std::path::PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let release_dir = dir.join("worlds").join("default");
            if release_dir.is_dir() {
                return release_dir;
            }
        }
    }
    std::path::PathBuf::from(DEFAULT_WORLD_DIR)
}

pub use file_system::{FileSystem, OsFileSystem};
pub use region_io::{load_region, save_region};
pub use types::{Compression, CompressionCodec, RegionEntry, WorldManifest};
pub use world_loader::{
    load_world_as_mesh_data, load_world_chunks, open_world_handle, save_edited_world,
    save_generated_world,
};
