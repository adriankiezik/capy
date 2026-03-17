use bevy_ecs::system::{NonSendMut, Res};

use crate::resources::trace::TracePipeline;
use crate::resources::{
    GpuContext, GtaoPipeline, RendererSettings, SharedVoxelBuffers, compute_scaled_resolution,
};

pub(crate) fn resize_gtao_system(
    gpu: NonSendMut<GpuContext>,
    trace: Option<NonSendMut<TracePipeline>>,
    gtao: Option<NonSendMut<GtaoPipeline>>,
    voxels: Option<Res<SharedVoxelBuffers>>,
    settings: Option<Res<RendererSettings>>,
) {
    let (Some(trace), Some(mut gtao), Some(voxels)) = (trace, gtao, voxels) else {
        return;
    };
    let scale = settings.as_deref().map_or(1.0, |s| s.render_scale);
    let (sw, sh) = compute_scaled_resolution(gpu.config.width, gpu.config.height, scale);
    if gtao.width != sw || gtao.height != sh {
        gtao.resize(
            &gpu.device,
            &trace.gbuf_depth,
            &trace.gbuf_normal,
            &voxels.camera_buffer,
            [sw, sh],
        );
    }
}
