use bevy_ecs::error::BevyError;
use bevy_ecs::system::{Res, ResMut};
use capy_core::{Camera, FrameTime, GameWindow, KeyCode, RawInput};

const LOOK_SENSITIVITY: f32 = 0.003;
const MOVE_SPEED_UNITS_PER_SEC: f32 = 80.0;

pub(crate) fn update_camera_system(
    mut camera: ResMut<Camera>,
    input: Res<RawInput>,
    time: Res<FrameTime>,
    window: Res<GameWindow>,
) -> Result<(), BevyError> {
    if window.height > 0 {
        camera.aspect = window.width as f32 / window.height as f32;
    }

    camera.yaw += input.mouse_dx * LOOK_SENSITIVITY;
    camera.pitch -= input.mouse_dy * LOOK_SENSITIVITY;
    camera.pitch = camera.pitch.clamp(
        -std::f32::consts::FRAC_PI_2 + 0.01,
        std::f32::consts::FRAC_PI_2 - 0.01,
    );

    let speed = MOVE_SPEED_UNITS_PER_SEC * time.dt;
    let fwd = camera.forward();
    let right = camera.right();

    if input.keys_held.contains(&KeyCode::KeyW) {
        camera.position += fwd * speed;
    }
    if input.keys_held.contains(&KeyCode::KeyS) {
        camera.position -= fwd * speed;
    }
    if input.keys_held.contains(&KeyCode::KeyD) {
        camera.position += right * speed;
    }
    if input.keys_held.contains(&KeyCode::KeyA) {
        camera.position -= right * speed;
    }
    if input.keys_held.contains(&KeyCode::Space) {
        camera.position.y += speed;
    }
    if input.keys_held.contains(&KeyCode::ShiftLeft) {
        camera.position.y -= speed;
    }

    Ok(())
}
