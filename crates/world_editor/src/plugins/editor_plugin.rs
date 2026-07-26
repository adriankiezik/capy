use bevy_ecs::schedule::{IntoScheduleConfigs, Schedules};
use bevy_ecs::world::World;
use capy_core::WindowConfig;
use capy_render::RendererSettings;

use capy_core::{PreviewGpuData, SelectionHighlight};

use crate::resources::path_state::PathState;
use crate::resources::{Clipboard, PreviewBake, SaveState, SelectionState};
use crate::systems;

pub struct EditorPlugin;

impl capy_core::Plugin for EditorPlugin {
    fn register(&self, world: &mut World) {
        world.insert_resource(WindowConfig {
            title: String::from("Capy World Editor"),
            width: 1600,
            height: 900,
            vsync: false,
        });
        let mut graphics = RendererSettings::with_palette(capy_core::MATERIAL_COLORS);
        graphics.render_scale = 0.25;
        graphics.sun_contribution = 0.0;
        graphics.vegetation_enabled = false;
        graphics.vegetation_shadow_enabled = false;
        graphics.water_enabled = false;
        graphics.water_reflections = false;
        graphics.water_shadows = false;
        world.insert_resource(graphics);
        #[cfg(feature = "dlss")]
        insert_dlss_settings(world);
        #[cfg(feature = "fsr")]
        world.insert_resource(capy_render::FsrSettings::default());
        world.insert_resource(SelectionState::default());
        world.insert_resource(Clipboard::default());
        world.insert_resource(SaveState::default());
        world.insert_resource(PreviewBake::default());
        world.insert_resource(PreviewGpuData::default());
        world.insert_resource(SelectionHighlight::default());
        world.insert_resource(PathState::default());

        let mut schedules = world.get_resource_or_init::<Schedules>();
        schedules
            .entry(capy_core::PreStartup)
            .add_systems(systems::editor_startup);
        schedules.entry(capy_core::Update).add_systems(
            (
                systems::input_edges,
                capy_shared::fly_camera_system,
                systems::shortcuts,
                systems::selection_system,
                systems::prefab_sync,
                systems::prefab_preview_bake,
                systems::path_tool,
                systems::edit_apply,
                systems::color_pick,
                systems::prefab_preview_position,
                systems::undo_redo,
                systems::editor_ui,
                capy_shared::graphics_settings_ui,
                systems::rebake,
                systems::world_save,
            )
                .chain(),
        );

        capy_render::ComputePassCallbacks::register_callback(
            world,
            systems::pick::pick_encode,
            Some(systems::pick::pick_post_submit),
        );
    }
}

#[cfg(feature = "dlss")]
fn insert_dlss_settings(world: &mut World) {
    let mut settings = capy_render::DlssSettings::default();
    // Allow overriding the project ID via environment variable.
    if let Ok(id) = std::env::var("CAPY_DLSS_PROJECT_ID") {
        if let Ok(parsed) = uuid::Uuid::parse_str(id.trim()) {
            settings.project_id = parsed;
        }
    }
    world.insert_resource(settings);
}
