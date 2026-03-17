use bevy_ecs::world::World;

use crate::resources::trace::TracePipeline;
use crate::resources::voxel_scene::VoxelSceneBuffers;
use crate::resources::{GpuContext, RendererSettings};

pub(crate) fn init_trace(world: &mut World) {
    let Some(scene) = world.get_non_send_resource::<VoxelSceneBuffers>() else {
        tracing::warn!("Missing VoxelSceneBuffers resource.");
        return;
    };

    let gpu = world.non_send_resource::<GpuContext>();
    let settings = world.resource::<RendererSettings>();
    let (sw, sh) = settings.scaled_resolution(gpu.config.width, gpu.config.height);

    let pipeline = TracePipeline::new(&gpu.device, sw, sh, scene);

    world.insert_non_send_resource(pipeline);
}
