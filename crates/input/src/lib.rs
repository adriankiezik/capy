mod plugins;
mod resources;

pub use plugins::InputPlugin;
pub use resources::CursorPosition;
pub(crate) use resources::{
    apply_keyboard_messages, apply_mouse_button_messages, apply_mouse_motion_messages,
    flush_input_system, init_input_resources, sync_cursor_mode_system, update_cursor_position,
};
