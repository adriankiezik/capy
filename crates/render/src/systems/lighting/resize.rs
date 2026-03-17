use bevy_ecs::system::{NonSendMut, Res};

use crate::resources::trace::TracePipeline;
use crate::resources::{
    GpuContext, GtaoPipeline, LightingPipeline, RendererSettings, SharedVoxelBuffers,
    compute_scaled_resolution,
};

pub(crate) fn resize_lighting_system(
    gpu: NonSendMut<GpuContext>,
    trace: Option<NonSendMut<TracePipeline>>,
    gtao: Option<NonSendMut<GtaoPipeline>>,
    lighting: Option<NonSendMut<LightingPipeline>>,
    voxels: Option<Res<SharedVoxelBuffers>>,
    settings: Option<Res<RendererSettings>>,
) {
    let (Some(trace), Some(gtao), Some(mut lighting), Some(voxels)) =
        (trace, gtao, lighting, voxels)
    else {
        return;
    };
    let scale = settings.as_deref().map_or(1.0, |s| s.render_scale);
    let (sw, sh) = compute_scaled_resolution(gpu.config.width, gpu.config.height, scale);
    if lighting.width != sw || lighting.height != sh {
        lighting.resize(
            &gpu.device,
            &trace.gbuf_color,
            &trace.gbuf_normal,
            &trace.gbuf_depth,
            &voxels.render_settings_buffer,
            &gtao.ao_output,
            [sw, sh],
        );
    }
}
