mod camera;
#[cfg(feature = "dlss")]
#[allow(dead_code, unused_imports)]
mod dlss;
mod error;
#[cfg(feature = "fsr")]
#[allow(dead_code, unused_imports)]
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

/// Add the `lib/` subdirectory next to the executable to the DLL search path.
///
/// Called automatically by `RenderPlugin::register` before any delay-loaded
/// vendor DLL (FSR, DLSS) is touched.
#[cfg(all(target_os = "windows", any(feature = "fsr", feature = "dlss")))]
pub(crate) fn add_lib_dll_search_path() {
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::System::LibraryLoader::{
        AddDllDirectory, LOAD_LIBRARY_SEARCH_DEFAULT_DIRS, SetDefaultDllDirectories,
    };

    // Enable AddDllDirectory by switching to the safe search-order mode.
    unsafe {
        let _ = SetDefaultDllDirectories(LOAD_LIBRARY_SEARCH_DEFAULT_DIRS);
    }

    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let Some(dir) = exe.parent() else { return };
    let lib_dir = dir.join("lib");
    if !lib_dir.is_dir() {
        return;
    }
    let wide: Vec<u16> = lib_dir
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        let _ = AddDllDirectory(windows::core::PCWSTR(wide.as_ptr()));
    }
}

pub use camera::{create_camera_buffer, write_camera_buffer};
pub use error::{RenderError, Result};
pub use plugins::RenderPlugin;
pub use resources::{
    ComputePassCallback, ComputePassCallbacks, ComputePassEncode, ComputePassPostSubmit, GpuAccess,
    MATERIAL_PALETTE_SIZE, PreparedVoxelSceneUpload, RenderOverlayCallback, RenderOverlayCallbacks,
    RendererSettings, SharedVoxelBuffers,
};
#[cfg(feature = "dlss")]
pub use resources::{DlssQualityMode, DlssSettings};
#[cfg(feature = "fsr")]
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
