use bevy_ecs::world::World;
use capy_core::FrameTime;
use capy_render::{RendererSettings, TonemappingMode};
use capy_ui::EguiContext;

#[cfg(feature = "dlss")]
use capy_render::{DlssQualityMode, DlssSettings};

#[cfg(feature = "fsr")]
use capy_render::{FsrQualityMode, FsrSettings};

pub fn graphics_settings_ui(world: &mut World) {
    let ctx = world.resource::<EguiContext>().0.clone();
    let dt = world.resource::<FrameTime>().dt;
    let mut settings = world.resource::<RendererSettings>().clone();

    #[cfg(feature = "dlss")]
    let mut dlss = world.get_resource::<DlssSettings>().cloned();

    #[cfg(feature = "fsr")]
    let mut fsr = world.get_resource::<FsrSettings>().cloned();

    #[cfg(feature = "dlss")]
    let dlss_fg_active = dlss.as_ref().is_some_and(|s| {
        s.enabled && s.supported && s.frame_generation_enabled && s.frame_generation_supported
    });
    #[cfg(not(feature = "dlss"))]
    let dlss_fg_active = false;

    #[cfg(feature = "fsr")]
    let fsr_fg_active = fsr.as_ref().is_some_and(|s| {
        s.enabled && s.supported && s.frame_generation_enabled && s.frame_generation_supported
    });
    #[cfg(not(feature = "fsr"))]
    let fsr_fg_active = false;

    let fg_active = dlss_fg_active || fsr_fg_active;

    egui::Window::new("Graphics")
        .default_open(false)
        .resizable(false)
        .show(&ctx, |ui| {
            fps_ui(ui, dt, fg_active);

            #[cfg(feature = "dlss")]
            let dlss_active = dlss.as_ref().is_some_and(|s| s.enabled && s.supported);
            #[cfg(not(feature = "dlss"))]
            let dlss_active = false;

            #[cfg(feature = "fsr")]
            let fsr_active = fsr.as_ref().is_some_and(|s| s.enabled && s.supported) && !dlss_active;
            #[cfg(not(feature = "fsr"))]
            let fsr_active = false;

            let upscaler_active = dlss_active || fsr_active;

            ui.collapsing("Upscalers", |ui| {
                #[cfg(feature = "dlss")]
                {
                    ui.label("DLSS");
                    dlss_ui(ui, &mut dlss, fsr_active);
                    ui.separator();
                }

                #[cfg(feature = "fsr")]
                {
                    ui.label("FSR 3.1");
                    fsr_ui(ui, &mut fsr, dlss_active);
                }

                if !upscaler_active {
                    ui.separator();
                    ui.label("Render Scale");
                    render_scale_ui(ui, &mut settings);
                }
            });

            ui.collapsing("Ambient Occlusion", |ui| {
                ao_settings_ui(ui, &mut settings);
            });

            ui.collapsing("Lighting", |ui| {
                lighting_ui(ui, &mut settings);
            });

            ui.collapsing("Vegetation", |ui| {
                vegetation_ui(ui, &mut settings);
            });

            ui.collapsing("Water", |ui| {
                water_ui(ui, &mut settings);
            });

            ui.collapsing("Post-Processing", |ui| {
                tonemapping_ui(ui, &mut settings);
            });
        });

    *world.resource_mut::<RendererSettings>() = settings;

    #[cfg(feature = "dlss")]
    if let Some(dlss) = dlss {
        if let Some(mut resource) = world.get_resource_mut::<DlssSettings>() {
            *resource = dlss;
        }
    }

    #[cfg(feature = "fsr")]
    if let Some(fsr) = fsr {
        if let Some(mut resource) = world.get_resource_mut::<FsrSettings>() {
            *resource = fsr;
        }
    }
}

fn fps_ui(ui: &mut egui::Ui, dt: f32, fg_active: bool) {
    let fps = if dt > 0.0 { 1.0 / dt } else { 0.0 };
    let smooth_fps = ui.memory_mut(|mem| {
        let id = egui::Id::new("fps_smooth");
        let prev: f32 = mem.data.get_temp(id).unwrap_or(fps);
        let smoothed = prev * 0.95 + fps * 0.05;
        mem.data.insert_temp(id, smoothed);
        smoothed
    });
    if fg_active {
        ui.label(format!(
            "FPS: {:.0} ({smooth_fps:.0} base)",
            smooth_fps * 2.0
        ));
    } else {
        ui.label(format!("FPS: {smooth_fps:.0}"));
    }
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

    ui.separator();

    // Reflex (low-latency mode)
    if settings.reflex_supported {
        ui.checkbox(&mut settings.reflex_enabled, "Reflex (Low Latency)");
    } else {
        ui.add_enabled(
            false,
            egui::Checkbox::new(&mut settings.reflex_enabled, "Reflex (Low Latency)"),
        );
        ui.weak("(not supported on this GPU)");
    }

    // Frame Generation (requires Reflex + RTX 4000+)
    if settings.frame_generation_supported {
        let can_enable = settings.reflex_enabled;
        ui.add_enabled(
            can_enable,
            egui::Checkbox::new(&mut settings.frame_generation_enabled, "Frame Generation"),
        );
        if !can_enable {
            ui.weak("(requires Reflex)");
        }
    } else {
        ui.add_enabled(
            false,
            egui::Checkbox::new(&mut settings.frame_generation_enabled, "Frame Generation"),
        );
        ui.weak("(not supported — requires RTX 4000+)");
    }

    true
}

#[cfg(feature = "fsr")]
fn fsr_ui(ui: &mut egui::Ui, fsr: &mut Option<FsrSettings>, dlss_active: bool) -> bool {
    let Some(settings) = fsr.as_mut() else {
        return false;
    };

    if !settings.supported {
        ui.checkbox(&mut settings.enabled, "Enabled");
        ui.weak("(not supported — DX12 backend required)");
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

    ui.separator();

    // Frame Generation
    if settings.frame_generation_supported {
        ui.checkbox(&mut settings.frame_generation_enabled, "Frame Generation");
    } else {
        ui.add_enabled(
            false,
            egui::Checkbox::new(&mut settings.frame_generation_enabled, "Frame Generation"),
        );
        ui.weak("(not supported — DX12 backend required)");
    }

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

fn ao_settings_ui(ui: &mut egui::Ui, settings: &mut RendererSettings) {
    let mut enabled = settings.ao_intensity > 0.0;
    if ui.checkbox(&mut enabled, "Enabled").changed() {
        settings.ao_intensity = if enabled { 1.0 } else { 0.0 };
    }
    if !enabled {
        return;
    }

    ui.add(egui::Slider::new(&mut settings.ao_radius, 0.5..=8.0).text("radius"));
    ui.add(egui::Slider::new(&mut settings.ao_intensity, 0.1..=4.0).text("intensity"));
    ui.add(egui::Slider::new(&mut settings.ao_samples, 1..=16).text("samples"));
    ui.add(egui::Slider::new(&mut settings.ao_steps, 1..=16).text("steps"));
}

fn vegetation_ui(ui: &mut egui::Ui, settings: &mut RendererSettings) {
    ui.checkbox(&mut settings.vegetation_enabled, "Enabled");
    if !settings.vegetation_enabled {
        return;
    }

    ui.add(
        egui::Slider::new(&mut settings.vegetation_density, 0.0..=4.0)
            .text("density")
            .fixed_decimals(2),
    );
    ui.add(
        egui::Slider::new(&mut settings.vegetation_length, 0.25..=3.0)
            .text("length")
            .fixed_decimals(2),
    );
    ui.add(
        egui::Slider::new(&mut settings.vegetation_scale, 0.25..=3.0)
            .text("blade scale")
            .fixed_decimals(2),
    );
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

fn water_ui(ui: &mut egui::Ui, settings: &mut RendererSettings) {
    ui.checkbox(&mut settings.water_enabled, "Enabled");
    if !settings.water_enabled {
        return;
    }

    ui.checkbox(&mut settings.water_reflections, "Ray-traced reflections");
    if settings.water_reflections {
        ui.add(
            egui::Slider::new(&mut settings.water_reflection_distance, 50.0..=1000.0)
                .text("reflection distance"),
        );
    }

    ui.checkbox(&mut settings.water_shadows, "Shadows");
    if settings.water_shadows {
        ui.add(
            egui::Slider::new(&mut settings.water_shadow_distance, 100.0..=4000.0)
                .text("shadow distance"),
        );
    }
}

fn tonemapping_ui(ui: &mut egui::Ui, settings: &mut RendererSettings) {
    ui.label("Tonemapping");
    ui.horizontal(|ui| {
        for mode in TonemappingMode::ALL {
            if ui
                .selectable_label(settings.tonemapping_mode == mode, mode.label())
                .clicked()
            {
                settings.tonemapping_mode = mode;
            }
        }
    });

    if settings.tonemapping_mode != TonemappingMode::Off {
        ui.add(
            egui::Slider::new(&mut settings.exposure, 0.1..=5.0)
                .text("exposure")
                .fixed_decimals(2),
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
