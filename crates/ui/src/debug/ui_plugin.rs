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
                style.interaction.selectable_labels = false;
                style.visuals.text_options.font_hinting = true;
                style.visuals.text_options.alpha_from_coverage =
                    egui::epaint::AlphaFromCoverage::Linear;
                for (_text_style, font_id) in style.text_styles.iter_mut() {
                    font_id.size *= 1.5;
                }
            });

            let mut fonts = egui::FontDefinitions::default();
            fonts.font_data.insert(
                "PixelifySans".to_owned(),
                std::sync::Arc::new(egui::FontData::from_static(include_bytes!(
                    "../../../../assets/fonts/PixelifySans-VariableFont_wght.ttf"
                ))),
            );
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .insert(0, "PixelifySans".to_owned());
            fonts
                .families
                .entry(egui::FontFamily::Monospace)
                .or_default()
                .insert(0, "PixelifySans".to_owned());
            ctx.set_fonts(fonts);

            world.insert_resource(EguiContext(ctx));
        }
    }
}
