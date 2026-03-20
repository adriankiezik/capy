use bevy_ecs::system::{NonSendMut, Res};

use crate::resources::trace::TracePipeline;
use crate::resources::{GpuContext, GtaoPipeline, RenderResolution, SharedVoxelBuffers};

pub(crate) fn resize_gtao_system(
    gpu: NonSendMut<GpuContext>,
    trace: Option<NonSendMut<TracePipeline>>,
    gtao: Option<NonSendMut<GtaoPipeline>>,
    voxels: Option<Res<SharedVoxelBuffers>>,
    resolution: Res<RenderResolution>,
) {
    let (Some(trace), Some(mut gtao), Some(voxels)) = (trace, gtao, voxels) else {
        return;
    };
    if gtao.width != resolution.width || gtao.height != resolution.height {
        gtao.resize(
            &gpu.device,
            &trace.gbuf_depth,
            &trace.gbuf_normal,
            &voxels.camera_buffer,
            [resolution.width, resolution.height],
        );
    }
}
