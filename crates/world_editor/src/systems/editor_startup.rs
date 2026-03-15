use std::path::Path;

use bevy_ecs::error::BevyError;
use bevy_ecs::world::World;
use capy_core::{Camera, CursorMode, GameWindow, MATERIAL_COLORS};
use capy_shared::FlyCameraConfig;
use glam::Vec3;

pub(crate) fn editor_startup(world: &mut World) -> Result<(), BevyError> {
    let world_dir = Path::new(capy_assets::DEFAULT_WORLD_DIR);

    if !world_dir.join("world.manifest").exists() {
        let baked = capy_world::generate_baked_terrain(42)?;
        capy_assets::save_generated_world(
            baked,
            capy_world::CHUNK_SIZE,
            MATERIAL_COLORS.to_vec(),
            world_dir,
        )?;
    }

    let mesh = capy_assets::load_world_as_mesh_data(world_dir)?;
    let handle = capy_assets::open_world_handle(world_dir)?;

    let window = world.resource::<GameWindow>();
    let aspect = if window.height > 0 {
        window.width as f32 / window.height as f32
    } else {
        1.0
    };

    world.insert_resource(mesh);
    world.insert_resource(Camera {
        position: Vec3::new(128.0, 120.0, -20.0),
        yaw: std::f32::consts::FRAC_PI_2,
        pitch: -0.2,
        aspect,
        ..Camera::default()
    });
    world.insert_resource(CursorMode::Confined);
    world.insert_resource(FlyCameraConfig::default());
    world.insert_resource(handle);

    Ok(())
}
