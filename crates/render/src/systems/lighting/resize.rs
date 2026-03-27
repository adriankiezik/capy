use bevy_ecs::system::{NonSendMut, Res};

use crate::resources::trace::TracePipeline;
use crate::resources::{
    GpuContext, GtaoPipeline, LightingPipeline, RenderResolution, RendererSettings,
    SharedVoxelBuffers,
};

pub(crate) fn resize_lighting_system(
    gpu: NonSendMut<GpuContext>,
    trace: Option<NonSendMut<TracePipeline>>,
    gtao: Option<NonSendMut<GtaoPipeline>>,
    lighting: Option<NonSendMut<LightingPipeline>>,
    voxels: Option<Res<SharedVoxelBuffers>>,
    resolution: Res<RenderResolution>,
    _settings: Res<RendererSettings>,
) {
    let (Some(trace), Some(gtao), Some(mut lighting), Some(voxels)) =
        (trace, gtao, lighting, voxels)
    else {
        return;
    };

    let ao_texture = &gtao.ao_output;

    if lighting.width != resolution.width || lighting.height != resolution.height {
        lighting.resize(
            &gpu.device,
            &trace.gbuf_color,
            &trace.gbuf_normal,
            &trace.gbuf_depth,
            &voxels.render_settings_buffer,
            ao_texture,
            &voxels.camera_buffer,
            [resolution.width, resolution.height],
        );
    }
}
