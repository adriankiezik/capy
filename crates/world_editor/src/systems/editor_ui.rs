use bevy_ecs::system::Res;
use capy_ui::EguiContext;

pub(crate) fn editor_ui(egui_ctx: Res<EguiContext>) {
    egui::SidePanel::left("editor_panel")
        .default_width(220.0)
        .show(&egui_ctx.0, |ui| {
            ui.heading("World Editor");
            ui.separator();
        });
}
