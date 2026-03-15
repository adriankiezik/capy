mod overlay;
pub(crate) mod resources;
mod runtime;
mod ui_plugin;

pub use overlay::render_egui_overlay;
pub use resources::EguiContext;
pub(crate) use resources::EguiPlatformState;
pub use resources::EguiRenderOutput;
pub(crate) use resources::UiEnabled;
pub use runtime::{
    begin_frame, end_frame, handle_window_event, initialize_platform, render_output,
    wants_pointer_input,
};
pub use ui_plugin::UiPlugin;
