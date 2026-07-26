use bevy_ecs::world::World;

use crate::resources::voxel_scene::VoxelSceneBuffers;
use crate::resources::{GpuContext, NearMeshPipeline};

pub(crate) fn init_near_mesh(world: &mut World) {
    let gpu = world.non_send_resource::<GpuContext>();
    let scene = world.non_send_resource::<VoxelSceneBuffers>();
    let pipeline = NearMeshPipeline::new(&gpu.device, scene);
    world.insert_non_send_resource(pipeline);
}
