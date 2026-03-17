use bevy_ecs::system::{Res, ResMut};
use capy_render::RendererSettings;
use capy_ui::EguiContext;

pub fn graphics_settings_ui(egui_ctx: Res<EguiContext>, mut settings: ResMut<RendererSettings>) {
    egui::Window::new("Graphics")
        .default_open(false)
        .resizable(false)
        .show(&egui_ctx.0, |ui| {
            render_scale_ui(ui, &mut settings);
            ui.separator();
            ao_settings_ui(ui, &mut settings);
            ui.separator();
            lighting_ui(ui, &mut settings);
        });
}

fn render_scale_ui(ui: &mut egui::Ui, settings: &mut RendererSettings) {
    ui.label("Render Scale");
    let mut scale = settings.render_scale;
    ui.horizontal(|ui| {
        if ui
            .selectable_label((scale - 1.0).abs() < 0.01, "1")
            .clicked()
        {
            scale = 1.0;
        }
        if ui
            .selectable_label((scale - 0.75).abs() < 0.01, "3/4")
            .clicked()
        {
            scale = 0.75;
        }
        if ui
            .selectable_label((scale - 0.5).abs() < 0.01, "1/2")
            .clicked()
        {
            scale = 0.5;
        }
        if ui
            .selectable_label((scale - 0.25).abs() < 0.01, "1/4")
            .clicked()
        {
            scale = 0.25;
        }
    });
    ui.add(egui::Slider::new(&mut scale, 0.1..=1.0).text("custom"));
    settings.render_scale = scale;
}

fn ao_settings_ui(ui: &mut egui::Ui, settings: &mut RendererSettings) {
    ui.label("Ambient Occlusion");
    ui.add(egui::Slider::new(&mut settings.ao_radius, 0.5..=8.0).text("radius"));
    ui.add(egui::Slider::new(&mut settings.ao_intensity, 0.1..=4.0).text("intensity"));
    ui.add(egui::Slider::new(&mut settings.ao_samples, 1..=16).text("samples"));
    ui.add(egui::Slider::new(&mut settings.ao_steps, 1..=16).text("steps"));
}

fn lighting_ui(ui: &mut egui::Ui, settings: &mut RendererSettings) {
    ui.label("Lighting");
    ui.add(egui::Slider::new(&mut settings.ambient_light, 0.0..=1.0).text("ambient"));
    ui.add(egui::Slider::new(&mut settings.sun_contribution, 0.0..=1.0).text("sun"));
}
