mod binary_io;
mod codec;
mod hash;
mod manifest;
mod region_io;
mod types;
mod world_handle;
mod world_loader;

pub const DEFAULT_WORLD_DIR: &str = "assets/worlds/default";

pub use region_io::{load_region, save_region};
pub use types::{Compression, RegionEntry, WorldManifest};
pub use world_handle::WorldHandle;
pub use world_loader::{load_world_as_mesh_data, open_world_handle, save_generated_world};
