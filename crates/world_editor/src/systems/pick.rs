use bevy_ecs::error::BevyError;
use bevy_ecs::world::World;

use capy_core::{Camera, GameWindow};
use capy_input::CursorPosition;
use capy_render::SharedVoxelBuffers;

use crate::resources::{PICK_OUTPUT_SIZE, PickInputUniform, PickPipeline, VoxelHit};

fn ensure_pick_pipeline(world: &mut World) {
    if world.get_non_send_resource::<PickPipeline>().is_some() {
        return;
    }

    let Some(gpu) = world.get_resource::<capy_render::GpuAccess>() else {
        return;
    };
    let Some(voxels) = world.get_resource::<SharedVoxelBuffers>() else {
        return;
    };
    let Some(camera) = world.get_resource::<Camera>() else {
        return;
    };
    let Some(window) = world.get_resource::<GameWindow>() else {
        return;
    };

    let pick = PickPipeline::new(&gpu.device, camera, window.width, window.height, voxels);
    world.insert_non_send_resource(pick);
    world.get_resource_or_init::<VoxelHit>();
}

pub(crate) fn pick_encode(
    world: &mut World,
    _device: &wgpu::Device,
    queue: &wgpu::Queue,
    encoder: &mut wgpu::CommandEncoder,
) -> Result<(), BevyError> {
    ensure_pick_pipeline(world);

    let Some(mut pick) = world.get_non_send_resource_mut::<PickPipeline>() else {
        return Ok(());
    };

    if let Some(result) = pick.try_read_result() {
        drop(pick);
        *world.resource_mut::<VoxelHit>() = result;
    } else {
        drop(pick);
    }

    let Some(cursor) = world.get_resource::<CursorPosition>() else {
        return Ok(());
    };
    let cursor_x = cursor.x;
    let cursor_y = cursor.y;

    let Some(window) = world.get_resource::<GameWindow>() else {
        return Ok(());
    };
    let win_width = window.width;
    let win_height = window.height;

    let pixel_x = (cursor_x as u32).min(win_width.saturating_sub(1));
    let pixel_y = (cursor_y as u32).min(win_height.saturating_sub(1));

    let Some(camera) = world.get_resource::<Camera>() else {
        return Ok(());
    };
    let camera_clone = *camera;

    let Some(pick) = world.get_non_send_resource::<PickPipeline>() else {
        return Ok(());
    };

    if pick.pending_rx.is_some() {
        return Ok(());
    }

    capy_render::write_camera_buffer(
        queue,
        &pick.camera_buffer,
        &camera_clone,
        win_width,
        win_height,
        0.0,
    );

    let pick_input = PickInputUniform { pixel_x, pixel_y };
    queue.write_buffer(&pick.pick_input_buffer, 0, bytemuck::bytes_of(&pick_input));

    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("Pick Pass"),
            ..Default::default()
        });
        pass.set_pipeline(&pick.compute_pipeline);
        pass.set_bind_group(0, &pick.bind_group, &[]);
        pass.dispatch_workgroups(1, 1, 1);
    }

    encoder.copy_buffer_to_buffer(
        &pick.pick_output_buffer,
        0,
        &pick.pick_staging_buffer,
        0,
        PICK_OUTPUT_SIZE,
    );

    Ok(())
}

pub(crate) fn pick_post_submit(world: &mut World) -> Result<(), BevyError> {
    let Some(mut pick) = world.get_non_send_resource_mut::<PickPipeline>() else {
        return Ok(());
    };

    if pick.pending_rx.is_some() {
        return Ok(());
    }

    let buffer_slice = pick.pick_staging_buffer.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = tx.send(result);
    });
    pick.pending_rx = Some(rx);

    Ok(())
}
