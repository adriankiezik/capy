use bevy_ecs::world::World;

use super::{EguiContext, UiEnabled};

pub struct UiPlugin;

impl capy_core::Plugin for UiPlugin {
    fn register(&self, world: &mut World) {
        world.insert_resource(UiEnabled);
        if world.get_resource::<EguiContext>().is_none() {
            world.insert_resource(EguiContext(egui::Context::default()));
        }
    }
}
