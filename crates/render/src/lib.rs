mod camera;
mod error;
mod plugins;
mod resources;
mod settings;
mod shader_source;
mod systems;
mod voxel_bind_group;

pub use camera::{create_camera_buffer, write_camera_buffer};
pub(crate) use error::{RenderError, Result};
pub use plugins::RenderPlugin;
pub use resources::{
    ComputePassCallback, ComputePassCallbacks, ComputePassEncode, ComputePassPostSubmit, GpuAccess,
    MATERIAL_PALETTE_SIZE, RenderOverlayCallback, RenderOverlayCallbacks, RendererSettings,
    SharedVoxelBuffers,
};
pub use shader_source::create_compute_shader;
pub use voxel_bind_group::{
    VOXEL_SCENE_BINDING_COUNT, bgl_storage_ro, bgl_storage_rw, bgl_uniform,
    voxel_scene_bind_group_entries, voxel_scene_bind_group_layout_entries,
};
