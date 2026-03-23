mod camera;
#[cfg(feature = "dlss")]
#[allow(dead_code, unused_imports)]
mod dlss;
mod error;
mod fsr;
mod gpu_texture;
mod pipeline_factory;
mod plugins;
mod resources;
mod settings;
mod shader_source;
mod systems;
mod uniform_buffer;
mod voxel_bind_group;

pub use camera::{create_camera_buffer, write_camera_buffer};
pub use error::{RenderError, Result};
pub use plugins::RenderPlugin;
pub use resources::{
    AoMode, ComputePassCallback, ComputePassCallbacks, ComputePassEncode, ComputePassPostSubmit,
    GpuAccess, MATERIAL_PALETTE_SIZE, PreparedVoxelSceneUpload, RenderOverlayCallback,
    RenderOverlayCallbacks, RendererSettings, SharedVoxelBuffers,
};
#[cfg(feature = "dlss")]
pub use resources::{DlssQualityMode, DlssSettings};
pub use resources::{FsrQualityMode, FsrSettings};
pub use shader_source::create_compute_shader;
pub use systems::voxel_scene::{
    apply_prepared_voxel_scene_upload, prepare_voxel_scene_upload, rebuild_voxel_scene,
};
pub use voxel_bind_group::{
    VOXEL_SCENE_BINDING_COUNT, bgl_sampled_texture, bgl_sampler_filtering, bgl_storage_ro,
    bgl_storage_rw, bgl_storage_texture, bgl_uniform, voxel_scene_bind_group_entries,
    voxel_scene_bind_group_layout_entries,
};
