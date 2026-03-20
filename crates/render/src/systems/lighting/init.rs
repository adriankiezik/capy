use bevy_ecs::world::World;

use crate::resources::trace::TracePipeline;
#[cfg(feature = "dlss")]
use crate::resources::{AoMode, RendererSettings, RtaoPipeline};
use crate::resources::{
    GpuContext, GtaoPipeline, LightingPipeline, RenderResolution, SharedVoxelBuffers,
};

pub(crate) fn init_lighting(world: &mut World) {
    let Some(trace) = world.get_non_send_resource::<TracePipeline>() else {
        tracing::warn!("Missing TracePipeline resource.");
        return;
    };

    let Some(gtao) = world.get_non_send_resource::<GtaoPipeline>() else {
        tracing::warn!("Missing GtaoPipeline resource.");
        return;
    };

    #[cfg(feature = "dlss")]
    let use_rtao = world
        .get_resource::<RendererSettings>()
        .is_some_and(|s| s.ao_mode == AoMode::RayTraced);

    #[cfg(feature = "dlss")]
    let ao_texture = if use_rtao {
        world
            .get_non_send_resource::<RtaoPipeline>()
            .map_or(&gtao.ao_output, |rtao| &rtao.ao_output)
    } else {
        &gtao.ao_output
    };
    #[cfg(not(feature = "dlss"))]
    let ao_texture = &gtao.ao_output;

    let voxels = world.resource::<SharedVoxelBuffers>();
    let gpu = world.non_send_resource::<GpuContext>();
    let resolution = world.resource::<RenderResolution>();

    #[cfg(feature = "dlss")]
    let pipeline = {
        let mut p = LightingPipeline::new(
            &gpu.device,
            &trace.gbuf_color,
            &trace.gbuf_normal,
            &trace.gbuf_depth,
            &voxels.render_settings_buffer,
            ao_texture,
            resolution.width,
            resolution.height,
        );
        p.ao_source_is_rtao = use_rtao;
        p
    };

    #[cfg(not(feature = "dlss"))]
    let pipeline = LightingPipeline::new(
        &gpu.device,
        &trace.gbuf_color,
        &trace.gbuf_normal,
        &trace.gbuf_depth,
        &voxels.render_settings_buffer,
        ao_texture,
        resolution.width,
        resolution.height,
    );

    world.insert_non_send_resource(pipeline);
}
