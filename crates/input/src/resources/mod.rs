mod input_state;

pub(crate) use input_state::{
    apply_keyboard_messages, apply_mouse_motion_messages, flush_input_system, init_input_resources,
    sync_cursor_mode_system,
};
