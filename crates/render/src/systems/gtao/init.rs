use bevy_ecs::world::World;

use crate::resources::trace::TracePipeline;
use crate::resources::{
    GpuContext, GtaoPipeline, RenderResolution, RendererSettings, SharedVoxelBuffers,
};

pub(crate) fn init_gtao(world: &mut World) {
    let Some(trace) = world.get_non_send_resource::<TracePipeline>() else {
        tracing::warn!("Missing TracePipeline resource.");
        return;
    };

    let voxels = world.resource::<SharedVoxelBuffers>();
    let gpu = world.non_send_resource::<GpuContext>();
    let settings = world.resource::<RendererSettings>();
    let resolution = world.resource::<RenderResolution>();

    let pipeline = GtaoPipeline::new(
        &gpu.device,
        &trace.gbuf_depth,
        &trace.gbuf_normal,
        &voxels.camera_buffer,
        resolution.width,
        resolution.height,
        settings,
    );

    world.insert_non_send_resource(pipeline);
}
