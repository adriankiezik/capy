use bevy_ecs::world::World;
use capy_core::{Camera, VoxelMeshData};

use crate::resources::{GpuContext, StreamingPipeline};

pub(crate) fn init_streaming(world: &mut World) {
    let Some(camera) = world.get_resource::<Camera>() else {
        tracing::warn!("Missing Camera resource.");
        return;
    };

    let empty_mesh;
    let mesh = match world.get_resource::<VoxelMeshData>() {
        Some(m) => m,
        None => {
            tracing::warn!("Missing VoxelMeshData resource — rendering empty void.");
            empty_mesh = VoxelMeshData::empty();
            &empty_mesh
        }
    };

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
