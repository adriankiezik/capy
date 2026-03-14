use std::collections::HashSet;

use bevy_ecs::resource::Resource;

use crate::key_code::KeyCode;

#[derive(Resource, Default)]
pub struct RawInput {
    pub keys_held: HashSet<KeyCode>,
    pub mouse_dx: f32,
    pub mouse_dy: f32,
}
