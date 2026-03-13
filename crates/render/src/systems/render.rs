use capy_core::{BevyError, NonSendMut};

use crate::resources::Renderer;

pub fn render_system(renderer: NonSendMut<Renderer>) -> Result<(), BevyError> {
    renderer.render()?;
    Ok(())
}
