mod binary_io;
mod codec;
mod file_system;
mod hash;
mod manifest;
mod region_io;
mod types;
mod world_loader;

pub const DEFAULT_WORLD_DIR: &str = "assets/worlds/default";

pub use file_system::{FileSystem, OsFileSystem};
pub use region_io::{load_region, save_region};
pub use types::{Compression, CompressionCodec, RegionEntry, WorldManifest};
pub use world_loader::{load_world_as_mesh_data, open_world_handle, save_generated_world};
