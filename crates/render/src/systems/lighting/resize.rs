use bevy_ecs::system::{NonSendMut, Res};

use crate::resources::trace::TracePipeline;
use crate::resources::{GpuContext, GtaoPipeline, LightingPipeline, SharedVoxelBuffers};

pub(crate) fn resize_lighting_system(
    gpu: NonSendMut<GpuContext>,
    trace: Option<NonSendMut<TracePipeline>>,
    gtao: Option<NonSendMut<GtaoPipeline>>,
    lighting: Option<NonSendMut<LightingPipeline>>,
    voxels: Option<Res<SharedVoxelBuffers>>,
) {
    let (Some(trace), Some(gtao), Some(mut lighting), Some(voxels)) =
        (trace, gtao, lighting, voxels)
    else {
        return;
    };
    if lighting.width != gpu.config.width || lighting.height != gpu.config.height {
        lighting.resize(
            &gpu.device,
            &trace.gbuf_color,
            &trace.gbuf_normal,
            &trace.gbuf_depth,
            &voxels.render_settings_buffer,
            &gtao.ao_output,
            [gpu.config.width, gpu.config.height],
        );
    }
}
