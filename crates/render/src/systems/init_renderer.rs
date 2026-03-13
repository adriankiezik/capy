use capy_core::{BevyError, GameWindow, World};

use crate::resources::Renderer;

/// Exclusive system (`&mut World`) because it inserts the renderer as a
/// non-send resource. This prevents parallel execution with other startup systems.
pub fn init_renderer(world: &mut World) -> Result<(), BevyError> {
    let window = world.resource::<GameWindow>();
    let handle = window.handle.clone();
    let width = window.width;
    let height = window.height;
    let renderer = Renderer::new(handle, width, height)?;
    world.insert_non_send_resource(renderer);
    Ok(())
}
