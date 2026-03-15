use bevy_ecs::change_detection::DetectChanges;
use bevy_ecs::system::{NonSendMut, Res};
use capy_core::Camera;

use crate::resources::voxel_scene::VoxelSceneBuffers;
use crate::resources::{GpuContext, RendererSettings};

pub(crate) fn upload_uniforms_system(
    gpu: NonSendMut<GpuContext>,
    scene: Option<NonSendMut<VoxelSceneBuffers>>,
    camera: Option<Res<Camera>>,
    settings: Option<Res<RendererSettings>>,
) {
    let Some(scene) = scene else {
        return;
    };

    if let Some(camera) = camera {
        let lod_bias = settings.as_deref().map_or(1.0, |s| s.lod_bias);
        scene.upload_camera(
            &gpu.queue,
            &camera,
            gpu.config.width,
            gpu.config.height,
            lod_bias,
        );
    }

    if let Some(settings) = settings {
        if settings.is_changed() {
            scene.upload_render_settings(&gpu.queue, &settings);
        }
    }
}
