use bevy_ecs::world::World;
use capy_core::FrameTime;
use capy_render::RendererSettings;
use capy_ui::EguiContext;

#[cfg(feature = "dlss")]
use capy_render::{AoMode, DlssQualityMode, DlssSettings};

use capy_render::{FsrQualityMode, FsrSettings};

pub fn graphics_settings_ui(world: &mut World) {
    let ctx = world.resource::<EguiContext>().0.clone();
    let dt = world.resource::<FrameTime>().dt;
    let mut settings = world.resource::<RendererSettings>().clone();

    #[cfg(feature = "dlss")]
    let mut dlss = world.get_resource::<DlssSettings>().cloned();

    let mut fsr = world.get_resource::<FsrSettings>().cloned();

    egui::Window::new("Graphics")
        .default_open(false)
        .resizable(false)
        .show(&ctx, |ui| {
            fps_ui(ui, dt);

            #[cfg(feature = "dlss")]
            let dlss_active = dlss.as_ref().is_some_and(|s| s.enabled && s.supported);
            #[cfg(not(feature = "dlss"))]
            let dlss_active = false;

            let fsr_active = fsr.as_ref().is_some_and(|s| s.enabled && s.supported);

            let upscaler_active = dlss_active || fsr_active;

            ui.collapsing("Upscalers", |ui| {
                #[cfg(feature = "dlss")]
                {
                    ui.label("DLSS");
                    dlss_ui(ui, &mut dlss, fsr_active);
                    ui.separator();
                }

                ui.label("FSR 2");
                fsr_ui(ui, &mut fsr, dlss_active);

                if !upscaler_active {
                    ui.separator();
                    ui.label("Render Scale");
                    render_scale_ui(ui, &mut settings);
                }
            });

            ui.collapsing("Ambient Occlusion", |ui| {
                #[cfg(feature = "dlss")]
                ao_settings_ui(ui, &mut settings, &dlss);
                #[cfg(not(feature = "dlss"))]
                ao_settings_ui(ui, &mut settings);
            });

            ui.collapsing("Lighting", |ui| {
                lighting_ui(ui, &mut settings);
            });

            ui.collapsing("Vegetation", |ui| {
                vegetation_ui(ui, &mut settings);
            });
        });

    *world.resource_mut::<RendererSettings>() = settings;

    #[cfg(feature = "dlss")]
    if let Some(dlss) = dlss {
        if let Some(mut resource) = world.get_resource_mut::<DlssSettings>() {
            *resource = dlss;
        }
    }

    if let Some(fsr) = fsr {
        if let Some(mut resource) = world.get_resource_mut::<FsrSettings>() {
            *resource = fsr;
        }
    }
}

fn fps_ui(ui: &mut egui::Ui, dt: f32) {
    let fps = if dt > 0.0 { 1.0 / dt } else { 0.0 };
    let smooth_fps = ui.memory_mut(|mem| {
        let id = egui::Id::new("fps_smooth");
        let prev: f32 = mem.data.get_temp(id).unwrap_or(fps);
        let smoothed = prev * 0.95 + fps * 0.05;
        mem.data.insert_temp(id, smoothed);
        smoothed
    });
    ui.label(format!("FPS: {smooth_fps:.0}"));
}

#[cfg(feature = "dlss")]
fn dlss_ui(ui: &mut egui::Ui, dlss: &mut Option<DlssSettings>, fsr_active: bool) -> bool {
    let Some(settings) = dlss.as_mut() else {
        return false;
    };

    if !settings.supported {
        ui.checkbox(&mut settings.enabled, "Enabled");
        ui.weak("(not supported on this GPU)");
        return false;
    }

    if fsr_active {
        settings.enabled = false;
        ui.add_enabled(false, egui::Checkbox::new(&mut settings.enabled, "Enabled"));
        return false;
    }

    ui.checkbox(&mut settings.enabled, "Enabled");

    if !settings.enabled {
        return false;
    }

    ui.horizontal(|ui| {
        let modes = [
            (DlssQualityMode::Auto, "Auto"),
            (DlssQualityMode::Dlaa, "DLAA"),
            (DlssQualityMode::Quality, "Quality"),
            (DlssQualityMode::Balanced, "Balanced"),
            (DlssQualityMode::Performance, "Perf"),
            (DlssQualityMode::UltraPerformance, "Ultra"),
        ];
        for (mode, label) in modes {
            if ui
                .selectable_label(settings.perf_quality == mode, label)
                .clicked()
            {
                settings.perf_quality = mode;
            }
        }
    });

    true
}

fn fsr_ui(ui: &mut egui::Ui, fsr: &mut Option<FsrSettings>, dlss_active: bool) -> bool {
    let Some(settings) = fsr.as_mut() else {
        return false;
    };

    if !settings.supported {
        ui.checkbox(&mut settings.enabled, "Enabled");
        ui.weak("(not supported — Vulkan backend required)");
        return false;
    }

    if dlss_active {
        // DLSS takes priority — disable FSR automatically.
        settings.enabled = false;
        ui.add_enabled(false, egui::Checkbox::new(&mut settings.enabled, "Enabled"));

        return false;
    }

    ui.checkbox(&mut settings.enabled, "Enabled");

    if !settings.enabled {
        return false;
    }

    ui.horizontal(|ui| {
        let modes = [
            (FsrQualityMode::Auto, "Auto"),
            (FsrQualityMode::NativeAA, "Native AA"),
            (FsrQualityMode::Quality, "Quality"),
            (FsrQualityMode::Balanced, "Balanced"),
            (FsrQualityMode::Performance, "Perf"),
            (FsrQualityMode::UltraPerformance, "Ultra"),
        ];
        for (mode, label) in modes {
            if ui
                .selectable_label(settings.quality == mode, label)
                .clicked()
            {
                settings.quality = mode;
            }
        }
    });

    true
}

fn render_scale_ui(ui: &mut egui::Ui, settings: &mut RendererSettings) {
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

#[cfg(feature = "dlss")]
fn ao_settings_ui(ui: &mut egui::Ui, settings: &mut RendererSettings, dlss: &Option<DlssSettings>) {
    let mut enabled = settings.ao_intensity > 0.0;
    if ui.checkbox(&mut enabled, "Enabled").changed() {
        settings.ao_intensity = if enabled { 1.0 } else { 0.0 };
    }
    if !enabled {
        return;
    }

    // Show AO mode selector when DLSS + Ray Reconstruction are available
    let rr_available = dlss
        .as_ref()
        .is_some_and(|d| d.enabled && d.supported && d.ray_reconstruction_supported);

    if rr_available {
        ui.horizontal(|ui| {
            ui.label("Mode:");
            if ui
                .selectable_label(settings.ao_mode == AoMode::ScreenSpace, "Screen-Space")
                .clicked()
            {
                settings.ao_mode = AoMode::ScreenSpace;
            }
            if ui
                .selectable_label(
                    settings.ao_mode == AoMode::RayTraced,
                    "Ray-Traced (unstable)",
                )
                .clicked()
            {
                settings.ao_mode = AoMode::RayTraced;
            }
        });
    } else if settings.ao_mode == AoMode::RayTraced {
        // Force back to screen-space if hardware doesn't support RR
        settings.ao_mode = AoMode::ScreenSpace;
    }

    if settings.ao_mode == AoMode::RayTraced {
        settings.ao_radius = 128.0;
        ui.add(egui::Slider::new(&mut settings.ao_intensity, 0.1..=3.0).text("intensity"));
        ui.add(egui::Slider::new(&mut settings.ao_rays, 1..=8).text("rays"));
    } else {
        ui.add(egui::Slider::new(&mut settings.ao_radius, 0.5..=8.0).text("radius"));
        ui.add(egui::Slider::new(&mut settings.ao_intensity, 0.1..=4.0).text("intensity"));
        ui.add(egui::Slider::new(&mut settings.ao_samples, 1..=16).text("samples"));
        ui.add(egui::Slider::new(&mut settings.ao_steps, 1..=16).text("steps"));
    }
}

#[cfg(not(feature = "dlss"))]
fn ao_settings_ui(ui: &mut egui::Ui, settings: &mut RendererSettings) {
    let mut enabled = settings.ao_intensity > 0.0;
    if ui.checkbox(&mut enabled, "Enabled").changed() {
        settings.ao_intensity = if enabled { 1.0 } else { 0.0 };
    }
    if enabled {
        ui.add(egui::Slider::new(&mut settings.ao_radius, 0.5..=8.0).text("radius"));
        ui.add(egui::Slider::new(&mut settings.ao_intensity, 0.1..=4.0).text("intensity"));
        ui.add(egui::Slider::new(&mut settings.ao_samples, 1..=16).text("samples"));
        ui.add(egui::Slider::new(&mut settings.ao_steps, 1..=16).text("steps"));
    }
}

fn vegetation_ui(ui: &mut egui::Ui, settings: &mut RendererSettings) {
    ui.checkbox(&mut settings.vegetation_enabled, "Enabled");
    if !settings.vegetation_enabled {
        return;
    }

    ui.add(egui::Slider::new(&mut settings.vegetation_density, 0.05..=1.0).text("density"));
    ui.add(
        egui::Slider::new(&mut settings.vegetation_max_distance, 8.0..=10_000.0).text("distance"),
    );
    ui.add(
        egui::Slider::new(&mut settings.vegetation_far_reduce_start, 8.0..=10_000.0)
            .text("far reduce start"),
    );
    ui.add(
        egui::Slider::new(&mut settings.vegetation_far_step_scale, 1.0..=4.0)
            .text("far step scale"),
    );
    ui.add(
        egui::Slider::new(&mut settings.vegetation_near_search_radius, 0..=2)
            .text("near search radius"),
    );
    ui.add(
        egui::Slider::new(&mut settings.vegetation_far_search_radius, 0..=2)
            .text("far search radius"),
    );

    ui.add(
        egui::Slider::new(&mut settings.vegetation_animation_distance, 0.0..=10_000.0)
            .text("animation distance"),
    );

    ui.checkbox(&mut settings.vegetation_shadow_enabled, "Grass shadows");
    if settings.vegetation_shadow_enabled {
        ui.add(
            egui::Slider::new(&mut settings.vegetation_shadow_distance, 1.0..=10_000.0)
                .text("shadow distance"),
        );
    }
}

fn lighting_ui(ui: &mut egui::Ui, settings: &mut RendererSettings) {
    ui.add(egui::Slider::new(&mut settings.ambient_light, 0.0..=1.0).text("ambient"));
    let mut sun_enabled = settings.sun_contribution > 0.0;
    if ui.checkbox(&mut sun_enabled, "Directional light").changed() {
        settings.sun_contribution = if sun_enabled { 0.8 } else { 0.0 };
    }
    if sun_enabled {
        ui.add(egui::Slider::new(&mut settings.sun_contribution, 0.01..=1.0).text("sun"));
    }
}
