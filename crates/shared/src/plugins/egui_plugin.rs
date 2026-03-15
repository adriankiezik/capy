use bevy_ecs::world::World;
use capy_window::{OnAppResumed, OnBeginFrame, OnEndFrame, OnWindowEvent, WantsPointerInput};

use crate::systems::render_ui_overlay;

pub struct EguiIntegrationPlugin;

impl capy_core::Plugin for EguiIntegrationPlugin {
    fn register(&self, world: &mut World) {
        capy_ui::UiPlugin.register(world);
        capy_render::RenderOverlayCallbacks::register_callback(world, render_ui_overlay);

        world
            .get_resource_or_init::<OnAppResumed>()
            .add(capy_ui::initialize_platform);
        world
            .get_resource_or_init::<OnWindowEvent>()
            .add(capy_ui::handle_window_event);
        world
            .get_resource_or_init::<OnBeginFrame>()
            .add(capy_ui::begin_frame);
        world
            .get_resource_or_init::<OnEndFrame>()
            .add(capy_ui::end_frame);
        world
            .get_resource_or_init::<WantsPointerInput>()
            .add(capy_ui::wants_pointer_input);
    }
}
