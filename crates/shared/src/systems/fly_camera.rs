use bevy_ecs::error::BevyError;
use bevy_ecs::system::{Res, ResMut};
use capy_core::{Camera, FrameTime, GameWindow, RawInput};

use crate::FlyCameraConfig;

pub fn fly_camera_system(
    mut camera: ResMut<Camera>,
    config: Res<FlyCameraConfig>,
    input: Res<RawInput>,
    time: Res<FrameTime>,
    window: Res<GameWindow>,
) -> Result<(), BevyError> {
    if window.height > 0 {
        camera.aspect = window.width as f32 / window.height as f32;
    }

    camera.yaw += input.mouse_dx * config.look_sensitivity;
    camera.pitch -= input.mouse_dy * config.look_sensitivity;
    camera.pitch = camera.pitch.clamp(
        -std::f32::consts::FRAC_PI_2 + 0.01,
        std::f32::consts::FRAC_PI_2 - 0.01,
    );

    let speed = config.move_speed * time.dt;
    let fwd = camera.forward();
    let right = camera.right();

    if input.keys_held.contains(&config.key_forward) {
        camera.position += fwd * speed;
    }
    if input.keys_held.contains(&config.key_back) {
        camera.position -= fwd * speed;
    }
    if input.keys_held.contains(&config.key_right) {
        camera.position += right * speed;
    }
    if input.keys_held.contains(&config.key_left) {
        camera.position -= right * speed;
    }
    if input.keys_held.contains(&config.key_up) {
        camera.position.y += speed;
    }
    if input.keys_held.contains(&config.key_down) {
        camera.position.y -= speed;
    }

    Ok(())
}
