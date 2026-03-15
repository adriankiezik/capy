use bevy_ecs::schedule::Schedules;
use bevy_ecs::world::World;
use capy_core::WindowConfig;

use crate::systems;

pub struct GamePlugin;

impl capy_core::Plugin for GamePlugin {
    fn register(&self, world: &mut World) {
        world.insert_resource(WindowConfig {
            title: String::from("Capy Engine"),
            width: 1280,
            height: 720,
            vsync: false,
            ..WindowConfig::default()
        });

        let mut schedules = world.get_resource_or_init::<Schedules>();
        schedules
            .entry(capy_core::PreStartup)
            .add_systems(systems::game_startup);
        schedules
            .entry(capy_core::Update)
            .add_systems(capy_shared::fly_camera_system);
    }
}
