use bevy_ecs::system::{NonSendMut, Res};

use crate::resources::trace::TracePipeline;
use crate::resources::voxel_scene::VoxelSceneBuffers;
use crate::resources::{GpuContext, RenderResolution, RendererSettings};
use crate::shader_source::TraceShaderFeatures;

pub(crate) fn resize_trace_system(
    gpu: NonSendMut<GpuContext>,
    scene: Option<NonSendMut<VoxelSceneBuffers>>,
    trace: Option<NonSendMut<TracePipeline>>,
    resolution: Res<RenderResolution>,
    settings: Res<RendererSettings>,
) {
    let (Some(scene), Some(mut trace)) = (scene, trace) else {
        return;
    };
    trace.update_features(
        &gpu.device,
        TraceShaderFeatures::from_settings(settings.as_ref()),
    );
    if trace.width != resolution.width || trace.height != resolution.height {
        trace.resize(&gpu.device, resolution.width, resolution.height, &scene);
    }
}
