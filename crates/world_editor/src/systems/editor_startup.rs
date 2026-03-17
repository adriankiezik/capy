use std::collections::HashMap;
use std::path::Path;

use bevy_ecs::error::BevyError;
use bevy_ecs::world::World;
use capy_core::{Camera, CursorMode, GameWindow, MATERIAL_COLORS, VoxelMeshData};
use capy_shared::FlyCameraConfig;
use glam::Vec3;
use tracing::info;

use crate::resources::{
    BakeTask, EditHistory, EditTask, EditableWorld, EditorState, InputEdge, MeshDirty,
    PendingEdits, PrefabLibrary, PrefabTask, WorldGrid,
};

const GRID_DIM_XZ: u32 = 32;

pub(crate) fn editor_startup(world: &mut World) -> Result<(), BevyError> {
    let canonical = capy_world::generate_flat_baked()?;

    // Try to load a previously saved world.
    let world_dir = Path::new(capy_assets::DEFAULT_WORLD_DIR);
    let edited_baked = match capy_assets::load_world_chunks(world_dir, &capy_assets::OsFileSystem) {
        Ok(chunks) if !chunks.is_empty() => {
            info!("[editor] loaded {} edited chunks from disk", chunks.len());
            chunks
        }
        _ => HashMap::new(),
    };

    let mesh = if edited_baked.is_empty() {
        VoxelMeshData::from_flat_world(
            &canonical,
            GRID_DIM_XZ,
            capy_world::CHUNK_XZ,
            capy_world::CHUNK_Y,
            MATERIAL_COLORS,
        )
    } else {
        VoxelMeshData::with_edited_chunks(
            &canonical,
            &edited_baked,
            GRID_DIM_XZ,
            capy_world::CHUNK_XZ,
            capy_world::CHUNK_Y,
            MATERIAL_COLORS,
        )
    };

    let window = world.resource::<GameWindow>();
    let aspect = if window.height > 0 {
        window.width as f32 / window.height as f32
    } else {
        1.0
    };

    world.insert_resource(mesh);
    world.insert_resource(Camera {
        position: Vec3::new(128.0, 180.0, -20.0),
        yaw: std::f32::consts::FRAC_PI_2,
        pitch: -0.2,
        aspect,
        ..Camera::default()
    });
    world.insert_resource(CursorMode::Free);
    world.insert_resource(FlyCameraConfig {
        hold_to_look: true,
        ..FlyCameraConfig::default()
    });
    world.insert_resource(WorldGrid {
        canonical_baked: canonical,
        edited_baked,
        grid_dim_xz: GRID_DIM_XZ,
    });
    world.insert_resource(EditableWorld::default());
    world.insert_resource(EditorState::default());
    world.insert_resource(EditHistory::default());
    world.insert_resource(MeshDirty::default());
    world.insert_resource(PendingEdits::default());
    world.insert_resource(PrefabLibrary::default());
    world.insert_non_send_resource(BakeTask::default());
    world.insert_non_send_resource(EditTask::default());
    world.insert_non_send_resource(PrefabTask::default());
    world.insert_resource(InputEdge::default());

    Ok(())
}
