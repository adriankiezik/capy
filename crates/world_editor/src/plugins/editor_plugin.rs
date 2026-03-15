use bevy_ecs::schedule::Schedules;
use bevy_ecs::world::World;
use capy_core::WindowConfig;

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

        let mut schedules = world.get_resource_or_init::<Schedules>();
        schedules
            .entry(capy_core::PreStartup)
            .add_systems(systems::editor_startup);
        schedules
            .entry(capy_core::Update)
            .add_systems((capy_shared::fly_camera_system, systems::editor_ui));

        capy_render::ComputePassCallbacks::register_callback(
            world,
            systems::pick::pick_encode,
            Some(systems::pick::pick_post_submit),
        );
    }
}
