use std::collections::HashSet;

use bevy_ecs::resource::Resource;
use capy_core::{KeyCode, MouseButton, RawInput};

#[derive(Resource, Default)]
pub struct InputEdge {
    prev_keys: HashSet<KeyCode>,
    prev_mouse: HashSet<MouseButton>,
    pub keys_just_pressed: HashSet<KeyCode>,
    pub mouse_just_pressed: HashSet<MouseButton>,
    pub mouse_just_released: HashSet<MouseButton>,
}

impl InputEdge {
    pub fn update(&mut self, input: &RawInput) {
        self.keys_just_pressed = input
            .keys_held
            .difference(&self.prev_keys)
            .copied()
            .collect();
        self.mouse_just_pressed = input
            .mouse_buttons_held
            .difference(&self.prev_mouse)
            .copied()
            .collect();
        self.mouse_just_released = self
            .prev_mouse
            .difference(&input.mouse_buttons_held)
            .copied()
            .collect();
        self.prev_keys = input.keys_held.clone();
        self.prev_mouse = input.mouse_buttons_held.clone();
    }
}
