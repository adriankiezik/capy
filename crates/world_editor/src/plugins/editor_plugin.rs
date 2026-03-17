use bevy_ecs::schedule::{IntoScheduleConfigs, Schedules};
use bevy_ecs::world::World;
use capy_core::WindowConfig;

use crate::resources::{Clipboard, SaveState, SelectionState};
use crate::systems;

pub struct EditorPlugin;

impl capy_core::Plugin for EditorPlugin {
    fn register(&self, world: &mut World) {
        world.insert_resource(WindowConfig {
            title: String::from("Capy World Editor"),
            width: 1600,
            height: 900,
            vsync: true,
        });
        world.insert_resource(SelectionState::default());
        world.insert_resource(Clipboard::default());
        world.insert_resource(SaveState::default());

        let mut schedules = world.get_resource_or_init::<Schedules>();
        schedules
            .entry(capy_core::PreStartup)
            .add_systems(systems::editor_startup);
        schedules.entry(capy_core::Update).add_systems(
            (
                systems::input_edges,
                capy_shared::fly_camera_system,
                systems::shortcuts,
                systems::selection_system,
                systems::prefab_sync,
                systems::edit_apply,
                systems::undo_redo,
                systems::editor_ui,
                systems::rebake,
                systems::world_save,
            )
                .chain(),
        );

        capy_render::ComputePassCallbacks::register_callback(
            world,
            systems::pick::pick_encode,
            Some(systems::pick::pick_post_submit),
        );
    }
}
