mod error;
mod resources;
mod systems;

pub(crate) use error::{RenderError, Result};
pub use systems::{init_renderer, render_system, resize_system};
