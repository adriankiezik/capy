use bevy_ecs::world::World;

use crate::resources::trace::TracePipeline;
use crate::resources::{GpuContext, LightingPipeline, SharedVoxelBuffers};

pub(crate) fn init_lighting(world: &mut World) {
    let Some(trace) = world.get_non_send_resource::<TracePipeline>() else {
        tracing::warn!("Missing TracePipeline resource.");
        return;
    };

    let voxels = world.resource::<SharedVoxelBuffers>();
    let gpu = world.non_send_resource::<GpuContext>();

    let pipeline = LightingPipeline::new(
        &gpu.device,
        &trace.gbuf_color,
        &trace.gbuf_normal,
        &trace.gbuf_depth,
        &voxels.render_settings_buffer,
        gpu.config.width,
        gpu.config.height,
    );

    world.insert_non_send_resource(pipeline);
}
