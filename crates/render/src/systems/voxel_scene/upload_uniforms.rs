use bevy_ecs::change_detection::DetectChanges;
#[cfg(feature = "dlss")]
use bevy_ecs::system::NonSend;
use bevy_ecs::system::{NonSendMut, Res, ResMut};
use capy_core::Camera;

use crate::camera::clip_from_world;
use crate::resources::voxel_scene::VoxelSceneBuffers;
#[cfg(feature = "dlss")]
use crate::resources::{DlssPipeline, DlssSettings};
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

        #[cfg(feature = "dlss")]
        let jitter = dlss
            .as_deref()
            .and_then(|dlss| dlss.suggested_jitter(temporal.frame_index()))
            .unwrap_or([0.0, 0.0]);
        #[cfg(not(feature = "dlss"))]
        let jitter = [0.0, 0.0];

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
