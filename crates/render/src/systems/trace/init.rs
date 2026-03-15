use bevy_ecs::world::World;

use crate::resources::GpuContext;
use crate::resources::trace::TracePipeline;
use crate::resources::voxel_scene::VoxelSceneBuffers;

pub(crate) fn init_trace(world: &mut World) {
    let Some(scene) = world.get_non_send_resource::<VoxelSceneBuffers>() else {
        tracing::warn!("Missing VoxelSceneBuffers resource.");
        return;
    };

    let gpu = world.non_send_resource::<GpuContext>();

    let pipeline = TracePipeline::new(&gpu.device, gpu.config.width, gpu.config.height, scene);

    world.insert_non_send_resource(pipeline);
}
