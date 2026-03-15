use std::collections::HashSet;

use bevy_ecs::resource::Resource;

use crate::input_messages::MouseButton;
use crate::key_code::KeyCode;

#[derive(Resource, Default)]
pub struct RawInput {
    pub keys_held: HashSet<KeyCode>,
    pub mouse_buttons_held: HashSet<MouseButton>,
    pub mouse_dx: f32,
    pub mouse_dy: f32,
}
