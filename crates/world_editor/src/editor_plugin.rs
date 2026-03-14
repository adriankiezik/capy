use bevy_ecs::schedule::Schedules;
use bevy_ecs::world::World;
use capy_core::{Camera, GameWindow, WindowConfig};

pub struct EditorPlugin;

impl capy_core::Plugin for EditorPlugin {
    fn register(&self, world: &mut World) {
        world.insert_resource(WindowConfig {
            title: String::from("Capy World Editor"),
            width: 1600,
            height: 900,
            vsync: true,
        });

        world
            .get_resource_or_init::<Schedules>()
            .entry(capy_core::PreStartup)
            .add_systems(editor_startup);
    }
}

fn editor_startup(world: &mut World) {
    let window = world.resource::<GameWindow>();
    let aspect = if window.height > 0 {
        window.width as f32 / window.height as f32
    } else {
        1.0
    };

    world.insert_resource(Camera {
        aspect,
        ..Camera::default()
    });
}
