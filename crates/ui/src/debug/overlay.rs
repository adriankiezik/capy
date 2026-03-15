use bevy_ecs::error::BevyError;
use bevy_ecs::world::World;

use super::resources::{EguiOverlayRenderer, EguiRenderOutput};

pub fn render_egui_overlay(
    world: &mut World,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    surface_format: wgpu::TextureFormat,
    encoder: &mut wgpu::CommandEncoder,
    output_view: &wgpu::TextureView,
    output: &EguiRenderOutput,
) -> Result<(), BevyError> {
    let clipped_primitives = &output.clipped_primitives;
    let textures_delta = &output.textures_delta;
    let pixels_per_point = output.pixels_per_point;
    let screen_size = output.screen_size;
    let should_recreate = world
        .get_non_send_resource::<EguiOverlayRenderer>()
        .map(|renderer| renderer.surface_format != surface_format)
        .unwrap_or(true);
    if should_recreate {
        world.insert_non_send_resource(EguiOverlayRenderer::new(device, surface_format));
    }

    let screen_descriptor = egui_wgpu::ScreenDescriptor {
        size_in_pixels: screen_size,
        pixels_per_point,
    };

    let mut renderer = world.non_send_resource_mut::<EguiOverlayRenderer>();
    for (id, image_delta) in &textures_delta.set {
        renderer
            .renderer
            .update_texture(device, queue, *id, image_delta);
    }

    let callback_command_buffers = renderer.renderer.update_buffers(
        device,
        queue,
        encoder,
        clipped_primitives,
        &screen_descriptor,
    );
    if !callback_command_buffers.is_empty() {
        queue.submit(callback_command_buffers);
    }

    {
        let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Egui Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: output_view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            ..Default::default()
        });
        let mut pass = pass.forget_lifetime();
        renderer
            .renderer
            .render(&mut pass, clipped_primitives, &screen_descriptor);
    }

    for id in &textures_delta.free {
        renderer.renderer.free_texture(id);
    }

    Ok(())
}
