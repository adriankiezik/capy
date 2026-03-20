use bevy_ecs::world::World;

use crate::resources::trace::TracePipeline;
use crate::resources::{
    GpuContext, RenderResolution, RendererSettings, RtaoPipeline, SharedVoxelBuffers,
};

pub(crate) fn init_rtao(world: &mut World) {
    let Some(trace) = world.get_non_send_resource::<TracePipeline>() else {
        tracing::warn!("Missing TracePipeline resource.");
        return;
    };

    let voxels = world.resource::<SharedVoxelBuffers>();
    let gpu = world.non_send_resource::<GpuContext>();
    let settings = world.resource::<RendererSettings>();
    let resolution = world.resource::<RenderResolution>();

    let pipeline = RtaoPipeline::new(
        &gpu.device,
        &trace.gbuf_depth,
        &trace.gbuf_normal,
        voxels,
        resolution.width,
        resolution.height,
        settings,
    );

    world.insert_non_send_resource(pipeline);
}
