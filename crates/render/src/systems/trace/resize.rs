use bevy_ecs::system::{NonSendMut, Res};

use crate::resources::trace::TracePipeline;
use crate::resources::voxel_scene::VoxelSceneBuffers;
use crate::resources::{GpuContext, RendererSettings, compute_scaled_resolution};

pub(crate) fn resize_trace_system(
    gpu: NonSendMut<GpuContext>,
    scene: Option<NonSendMut<VoxelSceneBuffers>>,
    trace: Option<NonSendMut<TracePipeline>>,
    settings: Option<Res<RendererSettings>>,
) {
    let (Some(scene), Some(mut trace)) = (scene, trace) else {
        return;
    };
    let scale = settings.as_deref().map_or(1.0, |s| s.render_scale);
    let (sw, sh) = compute_scaled_resolution(gpu.config.width, gpu.config.height, scale);
    if trace.width != sw || trace.height != sh {
        trace.resize(&gpu.device, sw, sh, &scene);
    }
}
