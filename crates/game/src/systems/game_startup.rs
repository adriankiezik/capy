use std::path::Path;

use bevy_ecs::error::BevyError;
use bevy_ecs::world::World;
use capy_core::{Camera, CursorMode, GameWindow, MATERIAL_COLORS, VoxelMeshData};
use capy_shared::FlyCameraConfig;
use glam::Vec3;

const GRID_DIM_XZ: u32 = 32;

pub(crate) fn game_startup(world: &mut World) -> Result<(), BevyError> {
    let world_dir = Path::new(capy_assets::DEFAULT_WORLD_DIR);
    let fs = capy_assets::OsFileSystem;
    let canonical = capy_world::generate_flat_baked()?;

    let mesh = match capy_assets::load_world_chunks(world_dir, &fs) {
        Ok(edited) if !edited.is_empty() => VoxelMeshData::with_edited_chunks(
            &canonical,
            &edited,
            GRID_DIM_XZ,
            capy_world::CHUNK_XZ,
            capy_world::CHUNK_Y,
            MATERIAL_COLORS,
        ),
        _ => VoxelMeshData::from_flat_world(
            &canonical,
            GRID_DIM_XZ,
            capy_world::CHUNK_XZ,
            capy_world::CHUNK_Y,
            MATERIAL_COLORS,
        ),
    };

    let window = world.resource::<GameWindow>();
    let aspect = if window.height > 0 {
        window.width as f32 / window.height as f32
    } else {
        1.0
    };

    let camera = Camera {
        position: Vec3::new(128.0, 180.0, -20.0),
        yaw: std::f32::consts::FRAC_PI_2,
        pitch: -0.2,
        aspect,
        ..Camera::default()
    };

    world.insert_resource(mesh);
    world.insert_resource(camera);
    world.insert_resource(CursorMode::Confined);
    world.insert_resource(FlyCameraConfig::default());

    Ok(())
}
