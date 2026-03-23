use bevy_ecs::system::{NonSendMut, Res};

use crate::resources::trace::TracePipeline;
#[cfg(feature = "dlss")]
use crate::resources::{AoMode, RtaoPipeline};
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
    settings: Res<RendererSettings>,
    #[cfg(feature = "dlss")] rtao: Option<NonSendMut<RtaoPipeline>>,
) {
    let (Some(trace), Some(gtao), Some(mut lighting), Some(voxels)) =
        (trace, gtao, lighting, voxels)
    else {
        return;
    };

    #[cfg(feature = "dlss")]
    let want_rtao = settings.ao_mode == AoMode::RayTraced && settings.ao_intensity > 0.0;
    #[cfg(not(feature = "dlss"))]
    let want_rtao = {
        let _ = &settings;
        false
    };

    #[cfg(feature = "dlss")]
    let ao_texture = if want_rtao {
        rtao.as_deref().map_or(&gtao.ao_output, |r| &r.ao_output)
    } else {
        &gtao.ao_output
    };
    #[cfg(not(feature = "dlss"))]
    let ao_texture = &gtao.ao_output;

    let ao_mode_changed = lighting.ao_source_is_rtao != want_rtao;

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
        lighting.ao_source_is_rtao = want_rtao;
    } else if ao_mode_changed {
        lighting.rebind_ao(
            &gpu.device,
            &trace.gbuf_color,
            &trace.gbuf_normal,
            &trace.gbuf_depth,
            &voxels.render_settings_buffer,
            ao_texture,
            &voxels.camera_buffer,
        );
        lighting.ao_source_is_rtao = want_rtao;
    }
}
