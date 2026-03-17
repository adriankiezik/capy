use bevy_ecs::world::World;
use capy_core::VoxelMeshData;

use crate::RenderError;
use crate::resources::GpuAccess;
use crate::resources::trace::TracePipeline;
use crate::resources::voxel_scene::{PreparedVoxelSceneUpload, VoxelSceneBuffers};

pub fn prepare_voxel_scene_upload(
    gpu: &GpuAccess,
    mesh: &VoxelMeshData,
) -> Result<PreparedVoxelSceneUpload, RenderError> {
    PreparedVoxelSceneUpload::build(&gpu.device, mesh)
}

pub fn apply_prepared_voxel_scene_upload(
    world: &mut World,
    mesh: &VoxelMeshData,
    upload: PreparedVoxelSceneUpload,
) -> bool {
    let (device, queue) = {
        let gpu = world.resource::<GpuAccess>();
        (gpu.device.clone(), gpu.queue.clone())
    };

    let buffers_recreated = {
        let mut scene = world.non_send_resource_mut::<VoxelSceneBuffers>();
        let buffers_recreated = scene.apply_prepared_upload(&queue, mesh, upload);
        if buffers_recreated {
            let shared = scene.shared_voxel_buffers();
            world.insert_resource(shared);
        }
        buffers_recreated
    };

    if buffers_recreated {
        let mut trace = world.remove_non_send_resource::<TracePipeline>();
        if let Some(ref mut trace) = trace {
            let scene = world.non_send_resource::<VoxelSceneBuffers>();
            trace.rebind(&device, scene);
        }
        if let Some(trace) = trace {
            world.insert_non_send_resource(trace);
        }
    }

    buffers_recreated
}

pub fn rebuild_voxel_scene(world: &mut World) -> Result<bool, RenderError> {
    let Some(mesh) = world.remove_resource::<VoxelMeshData>() else {
        return Ok(false);
    };

    let gpu = {
        let gpu = world.resource::<GpuAccess>();
        gpu.clone()
    };
    let upload = prepare_voxel_scene_upload(&gpu, &mesh)?;
    let buffers_recreated = apply_prepared_voxel_scene_upload(world, &mesh, upload);

    world.insert_resource(mesh);
    Ok(buffers_recreated)
}
