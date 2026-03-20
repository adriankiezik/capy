use bevy_ecs::world::World;

use super::{EguiContext, UiEnabled};

pub struct UiPlugin;

impl capy_core::Plugin for UiPlugin {
    fn register(&self, world: &mut World) {
        world.insert_resource(UiEnabled);
        if world.get_resource::<EguiContext>().is_none() {
            let ctx = egui::Context::default();
            ctx.global_style_mut(|style| {
                style.visuals.window_shadow = egui::Shadow::NONE;
                style.visuals.popup_shadow = egui::Shadow::NONE;
            });
            world.insert_resource(EguiContext(ctx));
        }
    }
}
