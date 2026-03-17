use bevy_ecs::world::World;

use crate::resources::trace::TracePipeline;
use crate::resources::{
    GpuContext, GtaoPipeline, LightingPipeline, RendererSettings, SharedVoxelBuffers,
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

    let voxels = world.resource::<SharedVoxelBuffers>();
    let gpu = world.non_send_resource::<GpuContext>();
    let settings = world.resource::<RendererSettings>();
    let (sw, sh) = settings.scaled_resolution(gpu.config.width, gpu.config.height);

    let pipeline = LightingPipeline::new(
        &gpu.device,
        &trace.gbuf_color,
        &trace.gbuf_normal,
        &trace.gbuf_depth,
        &voxels.render_settings_buffer,
        &gtao.ao_output,
        sw,
        sh,
    );

    world.insert_non_send_resource(pipeline);
}
