use std::time::Instant;

use bevy_ecs::error::BevyError;
use bevy_ecs::world::World;
use capy_core::{KeyCode, MATERIAL_COLORS, RawInput};
use tracing::{error, info};

use crate::resources::{BakeTask, InputEdge, SaveResult, SaveState, WorldGrid};

pub(crate) fn world_save(world: &mut World) -> Result<(), BevyError> {
    // Detect Ctrl+S
    {
        let input = world.resource::<RawInput>();
        let edge = world.resource::<InputEdge>();
        let ctrl_held = input.keys_held.contains(&KeyCode::ControlLeft)
            || input.keys_held.contains(&KeyCode::ControlRight);
        let s_pressed = edge.keys_just_pressed.contains(&KeyCode::KeyS);
        if ctrl_held && s_pressed {
            world.resource_mut::<SaveState>().requested = true;
        }
    }

    if !world.resource::<SaveState>().requested {
        return Ok(());
    }

    // Wait for any pending bake — edited_baked is taken during rebake.
    if world.non_send_resource::<BakeTask>().pending.is_some() {
        return Ok(());
    }

    // Perform the save.
    let world_dir = capy_assets::resolve_world_dir();
    let (result, count) = {
        let grid = world.resource::<WorldGrid>();
        let count = grid.edited_baked.len();
        let result = capy_assets::save_edited_world(
            &grid.edited_baked,
            capy_world::CHUNK_XZ,
            &MATERIAL_COLORS,
            &world_dir,
            &capy_assets::OsFileSystem,
        );
        (result, count)
    };

    let mut state = world.resource_mut::<SaveState>();
    state.requested = false;
    match result {
        Ok(()) => {
            info!(
                "[save] saved {count} edited chunks to {}",
                world_dir.display()
            );
            state.last_save = Some((Instant::now(), SaveResult::Success(count)));
        }
        Err(e) => {
            error!("[save] failed: {e}");
            state.last_save = Some((Instant::now(), SaveResult::Error(e.to_string())));
        }
    }

    Ok(())
}
