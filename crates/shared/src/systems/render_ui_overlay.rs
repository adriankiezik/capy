use bevy_ecs::error::BevyError;
use bevy_ecs::world::World;

pub(crate) fn render_ui_overlay(
    world: &mut World,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    surface_format: wgpu::TextureFormat,
    encoder: &mut wgpu::CommandEncoder,
    output_view: &wgpu::TextureView,
) -> Result<(), BevyError> {
    let Some(output) = capy_ui::render_output(world) else {
        return Ok(());
    };

    capy_ui::render_egui_overlay(
        world,
        device,
        queue,
        surface_format,
        encoder,
        output_view,
        &output,
    )
}
