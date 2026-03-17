use bevy_ecs::system::{NonSendMut, Res};

use crate::resources::trace::TracePipeline;
use crate::resources::{GpuContext, GtaoPipeline, SharedVoxelBuffers};

pub(crate) fn resize_gtao_system(
    gpu: NonSendMut<GpuContext>,
    trace: Option<NonSendMut<TracePipeline>>,
    gtao: Option<NonSendMut<GtaoPipeline>>,
    voxels: Option<Res<SharedVoxelBuffers>>,
) {
    let (Some(trace), Some(mut gtao), Some(voxels)) = (trace, gtao, voxels) else {
        return;
    };
    if gtao.width != gpu.config.width || gtao.height != gpu.config.height {
        gtao.resize(
            &gpu.device,
            &trace.gbuf_depth,
            &trace.gbuf_normal,
            &voxels.camera_buffer,
            [gpu.config.width, gpu.config.height],
        );
    }
}
