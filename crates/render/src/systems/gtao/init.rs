use bevy_ecs::world::World;

use crate::resources::trace::TracePipeline;
use crate::resources::{GpuContext, GtaoPipeline, RendererSettings, SharedVoxelBuffers};

pub(crate) fn init_gtao(world: &mut World) {
    let Some(trace) = world.get_non_send_resource::<TracePipeline>() else {
        tracing::warn!("Missing TracePipeline resource.");
        return;
    };

    let voxels = world.resource::<SharedVoxelBuffers>();
    let gpu = world.non_send_resource::<GpuContext>();
    let settings = world.resource::<RendererSettings>();
    let (sw, sh) = settings.scaled_resolution(gpu.config.width, gpu.config.height);

    let pipeline = GtaoPipeline::new(
        &gpu.device,
        &trace.gbuf_depth,
        &trace.gbuf_normal,
        &voxels.camera_buffer,
        sw,
        sh,
        settings,
    );

    world.insert_non_send_resource(pipeline);
}
