use bevy_ecs::world::World;

use crate::resources::{BlitPipeline, GpuContext, StreamingPipeline};

pub(crate) fn init_blit(world: &mut World) {
    let Some(streaming) = world.get_non_send_resource::<StreamingPipeline>() else {
        tracing::warn!("Missing StreamingPipeline resource.");
        return;
    };

    let gpu = world.non_send_resource::<GpuContext>();

    let pipeline = BlitPipeline::new(&gpu.device, &streaming.storage_texture, gpu.config.format);

    world.insert_non_send_resource(pipeline);
}
