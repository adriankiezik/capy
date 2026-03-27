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
    let Some(mut output) = capy_ui::render_output(world) else {
        return Ok(());
    };

    let result = capy_ui::render_egui_overlay(
        world,
        device,
        queue,
        surface_format,
        encoder,
        output_view,
        &output,
    );

    // Re-insert with cleared texture deltas so a second render pass
    // (e.g. FG double-present) can draw the same primitives without
    // re-uploading or double-freeing textures.
    output.textures_delta.set.clear();
    output.textures_delta.free.clear();
    world.insert_non_send_resource(output);

    result
}
