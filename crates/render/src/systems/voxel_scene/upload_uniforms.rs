use bevy_ecs::change_detection::DetectChanges;
#[cfg(any(feature = "dlss", feature = "fsr"))]
use bevy_ecs::system::NonSend;
use bevy_ecs::system::{NonSendMut, Res, ResMut};
use capy_core::Camera;

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
    mut temporal: ResMut<TemporalCameraState>,
    #[cfg(feature = "dlss")] dlss: Option<NonSend<DlssPipeline>>,
    #[cfg(feature = "dlss")] dlss_settings: Option<Res<DlssSettings>>,
    #[cfg(feature = "fsr")] fsr: Option<NonSend<FsrPipeline>>,
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
        let mut jitter = [0.0, 0.0];
        #[cfg(feature = "dlss")]
        {
            if let Some(dlss_jitter) = dlss
                .as_deref()
                .and_then(|dlss| dlss.suggested_jitter(temporal.frame_index()))
            {
                jitter = dlss_jitter;
            }
        }
        #[cfg(feature = "fsr")]
        if jitter == [0.0, 0.0] {
            if let Some(fsr_jitter) = fsr
                .as_deref()
                .and_then(|fsr| fsr.suggested_jitter(temporal.frame_index()))
            {
                jitter = fsr_jitter;
            }
        }

        let current_clip_from_world = clip_from_world(&camera).to_cols_array();
        let previous_clip_from_world = temporal.previous_clip_from_world(current_clip_from_world);

        scene.upload_camera(
            &gpu.queue,
            &camera,
            render_resolution.width,
            render_resolution.height,
            lod_bias,
            jitter,
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
}
