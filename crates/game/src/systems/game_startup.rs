use bevy_ecs::error::BevyError;
use bevy_ecs::world::World;
use capy_core::{Camera, CursorMode, GameWindow};
use glam::Vec3;

pub(crate) fn game_startup(world: &mut World) -> Result<(), BevyError> {
    let mesh = capy_world::generate_terrain(42)?;

    let window = world.resource::<GameWindow>();
    let aspect = if window.height > 0 {
        window.width as f32 / window.height as f32
    } else {
        1.0
    };

    let camera = Camera {
        position: Vec3::new(128.0, 120.0, -20.0),
        yaw: std::f32::consts::FRAC_PI_2,
        pitch: -0.2,
        aspect,
        ..Camera::default()
    };

    world.insert_resource(mesh);
    world.insert_resource(camera);
    world.insert_resource(CursorMode::Confined);

    Ok(())
}
