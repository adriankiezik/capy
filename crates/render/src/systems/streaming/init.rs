use bevy_ecs::world::World;
use capy_core::{Camera, VoxelMeshData};

use crate::resources::{GpuContext, StreamingPipeline};

pub(crate) fn init_streaming(world: &mut World) {
    let mesh = world.resource::<VoxelMeshData>();
    let camera = world.resource::<Camera>();
    let gpu = world.non_send_resource::<GpuContext>();

    let pipeline = StreamingPipeline::new(
        &gpu.device,
        gpu.config.width,
        gpu.config.height,
        mesh,
        camera,
    );

    world.insert_non_send_resource(pipeline);
}
