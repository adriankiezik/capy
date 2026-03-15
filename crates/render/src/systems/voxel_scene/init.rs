use bevy_ecs::world::World;
use capy_core::{Camera, VoxelMeshData};

use crate::resources::voxel_scene::VoxelSceneBuffers;
use crate::resources::{GpuContext, RendererSettings};

pub(crate) fn init_voxel_scene(world: &mut World) {
    if world.get_resource::<Camera>().is_none() {
        tracing::warn!("Missing Camera resource.");
        return;
    }

    let has_mesh = world.get_resource::<VoxelMeshData>().is_some();
    if !has_mesh {
        tracing::warn!("Missing VoxelMeshData resource — rendering empty void.");
        world.insert_resource(VoxelMeshData::empty());
    }

    if world.get_resource::<RendererSettings>().is_none() {
        let palette = world.resource::<VoxelMeshData>().material_palette;
        world.insert_resource(RendererSettings::with_palette(palette));
    }

    let camera = world.resource::<Camera>();
    let mesh = world.resource::<VoxelMeshData>();
    let settings = world.resource::<RendererSettings>();
    let gpu = world.non_send_resource::<GpuContext>();

    let scene = VoxelSceneBuffers::new(
        &gpu.device,
        mesh,
        camera,
        gpu.config.width,
        gpu.config.height,
        settings,
    );

    world.insert_resource(scene.shared_voxel_buffers());
    world.insert_non_send_resource(scene);
}
