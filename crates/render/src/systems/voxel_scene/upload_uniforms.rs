use bevy_ecs::change_detection::DetectChanges;
use bevy_ecs::system::{NonSend, NonSendMut, Res, ResMut};
use capy_core::{Camera, PreviewGpuData, SelectionHighlight, VoxelMeshData};

use crate::camera::clip_from_world;
use crate::resources::voxel_scene::VoxelSceneBuffers;
#[cfg(feature = "dlss")]
use crate::resources::{DlssPipeline, DlssSettings};
#[cfg(feature = "fsr")]
use crate::resources::{FsrPipeline, FsrSettings};
use crate::resources::{
    GpuContext, GtaoPipeline, RenderResolution, RendererSettings, TemporalCameraState,
};

pub(crate) fn upload_uniforms_system(
    gpu: NonSendMut<GpuContext>,
    scene: Option<NonSendMut<VoxelSceneBuffers>>,
    gtao: Option<NonSendMut<GtaoPipeline>>,
    camera: Option<Res<Camera>>,
    render_resolution: Res<RenderResolution>,
    settings: Option<Res<RendererSettings>>,
    mesh: Option<Res<VoxelMeshData>>,
    mut temporal: ResMut<TemporalCameraState>,
    preview_gpu: Option<Res<PreviewGpuData>>,
    selection_highlight: Option<Res<SelectionHighlight>>,
    #[cfg(feature = "dlss")] dlss: Option<NonSend<DlssPipeline>>,
    #[cfg(feature = "dlss")] dlss_settings: Option<Res<DlssSettings>>,
    #[cfg(feature = "fsr")] mut fsr: Option<NonSendMut<FsrPipeline>>,
    #[cfg(feature = "fsr")] fsr_settings: Option<Res<FsrSettings>>,
) {
    let Some(scene) = scene else {
        return;
    };

    if let Some(camera) = camera {
        let lod_bias = settings.as_deref().map_or(1.0, |s| s.lod_bias);
        #[cfg(feature = "dlss")]
        if dlss_settings
            .as_deref()
            .is_some_and(|settings| settings.reset)
        {
            temporal.reset_history();
        }
        #[cfg(feature = "fsr")]
        if fsr_settings
            .as_deref()
            .is_some_and(|settings| settings.reset)
        {
            temporal.reset_history();
        }

        // Get jitter from the active upscaler. DLSS takes priority over FSR.
        #[allow(unused_mut)]
        let mut jitter = [0.0, 0.0];
        #[allow(unused_mut)]
        let mut temporal_outputs_enabled = false;
        #[cfg(feature = "dlss")]
        {
            if let Some(dlss_jitter) = dlss
                .as_deref()
                .and_then(|dlss| dlss.suggested_jitter(temporal.frame_index()))
            {
                jitter = dlss_jitter;
                temporal_outputs_enabled = true;
            }
        }
        #[cfg(feature = "fsr")]
        if jitter == [0.0, 0.0] {
            if let Some(fsr_jitter) = fsr
                .as_deref_mut()
                .and_then(|fsr| fsr.suggested_jitter(temporal.frame_index()))
            {
                jitter = fsr_jitter;
                temporal_outputs_enabled = true;
            }
        }

        let current_clip_from_world = clip_from_world(&camera).to_cols_array();
        let previous_clip_from_world = temporal.previous_clip_from_world(current_clip_from_world);
        let water_enabled = settings.as_deref().map_or(true, |s| s.water_enabled);
        let camera_underwater = water_enabled
            && mesh
                .as_deref()
                .is_some_and(|mesh| mesh.is_water_at(camera.position.to_array()));

        scene.upload_camera(
            &gpu.queue,
            &camera,
            render_resolution.width,
            render_resolution.height,
            lod_bias,
            camera_underwater,
            jitter,
            temporal_outputs_enabled,
            previous_clip_from_world,
            temporal.elapsed_secs(),
        );
        temporal.set_current_frame(current_clip_from_world, jitter);
    }

    if let Some(settings) = settings
        && settings.is_changed()
    {
        scene.upload_render_settings(&gpu.queue, &settings);
        if let Some(gtao) = gtao {
            gtao.update_params(&gpu.queue, &settings);
        }
    }

    if let Some(preview) = preview_gpu {
        scene.upload_preview_params(&gpu.queue, &preview);
    }

    if let Some(sel) = selection_highlight {
        scene.upload_selection(&gpu.queue, &sel);
    }
}
