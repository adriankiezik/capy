use bevy_ecs::system::NonSendMut;

use crate::resources::GpuContext;
use crate::resources::trace::TracePipeline;
use crate::resources::voxel_scene::VoxelSceneBuffers;

pub(crate) fn resize_trace_system(
    gpu: NonSendMut<GpuContext>,
    scene: Option<NonSendMut<VoxelSceneBuffers>>,
    trace: Option<NonSendMut<TracePipeline>>,
) {
    let (Some(scene), Some(mut trace)) = (scene, trace) else {
        return;
    };
    if trace.width != gpu.config.width || trace.height != gpu.config.height {
        trace.resize(&gpu.device, gpu.config.width, gpu.config.height, &scene);
    }
}
