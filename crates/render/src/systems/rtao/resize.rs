use bevy_ecs::system::{NonSendMut, Res};

use crate::resources::trace::TracePipeline;
use crate::resources::{GpuContext, RenderResolution, RtaoPipeline, SharedVoxelBuffers};

pub(crate) fn resize_rtao_system(
    gpu: NonSendMut<GpuContext>,
    trace: Option<NonSendMut<TracePipeline>>,
    rtao: Option<NonSendMut<RtaoPipeline>>,
    voxels: Option<Res<SharedVoxelBuffers>>,
    resolution: Res<RenderResolution>,
) {
    let (Some(trace), Some(mut rtao), Some(voxels)) = (trace, rtao, voxels) else {
        return;
    };
    if rtao.width != resolution.width || rtao.height != resolution.height {
        rtao.resize(
            &gpu.device,
            &trace.gbuf_depth,
            &trace.gbuf_normal,
            &voxels,
            [resolution.width, resolution.height],
        );
    }
}
