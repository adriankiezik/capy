use bevy_ecs::world::World;

use crate::resources::{BlitPipeline, GpuContext, LightingPipeline};

pub(crate) fn init_blit(world: &mut World) {
    let Some(lighting) = world.get_non_send_resource::<LightingPipeline>() else {
        tracing::warn!("Missing LightingPipeline resource.");
        return;
    };

    let gpu = world.non_send_resource::<GpuContext>();

    let pipeline = BlitPipeline::new(
        &gpu.device,
        &lighting.output_color,
        gpu.config.format,
        gpu.config.width,
        gpu.config.height,
    );

    world.insert_non_send_resource(pipeline);
}
