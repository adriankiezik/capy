use bevy_ecs::resource::Resource;

use capy_core::KeyCode;

#[derive(Resource)]
pub struct FlyCameraConfig {
    pub look_sensitivity: f32,
    pub move_speed: f32,
    pub key_forward: KeyCode,
    pub key_back: KeyCode,
    pub key_left: KeyCode,
    pub key_right: KeyCode,
    pub key_up: KeyCode,
    pub key_down: KeyCode,
}

impl Default for FlyCameraConfig {
    fn default() -> Self {
        Self {
            look_sensitivity: 0.003,
            move_speed: 80.0,
            key_forward: KeyCode::KeyW,
            key_back: KeyCode::KeyS,
            key_left: KeyCode::KeyA,
            key_right: KeyCode::KeyD,
            key_up: KeyCode::Space,
            key_down: KeyCode::ShiftLeft,
        }
    }
}
