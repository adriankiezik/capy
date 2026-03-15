use crate::KeyCode;

#[derive(bevy_ecs::prelude::Message, Debug, Clone, Copy, PartialEq)]
pub struct KeyboardInputMessage {
    pub key: KeyCode,
    pub pressed: bool,
}

#[derive(bevy_ecs::prelude::Message, Debug, Clone, Copy, PartialEq)]
pub struct MouseMotionMessage {
    pub dx: f64,
    pub dy: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

#[derive(bevy_ecs::prelude::Message, Debug, Clone, Copy, PartialEq)]
pub struct MouseButtonMessage {
    pub button: MouseButton,
    pub pressed: bool,
}
