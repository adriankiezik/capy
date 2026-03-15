use bevy_ecs::world::World;
use capy_core::Plugin;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::Window;

use crate::systems::render_ui_overlay;

/// Engine plugin that provides full egui integration.
///
/// Handles platform initialization, event forwarding, frame lifecycle,
/// and render overlay registration. Binaries that want egui just add this
/// plugin via `EngineBuilder::add_plugin`.
pub struct UiEnginePlugin;

impl capy_engine::EnginePlugin for UiEnginePlugin {
    fn register(&self, world: &mut World) {
        capy_ui::UiPlugin.register(world);
        capy_render::RenderOverlayCallbacks::register_callback(world, render_ui_overlay);
    }

    fn on_app_resumed(&self, world: &mut World, event_loop: &ActiveEventLoop) {
        capy_ui::initialize_platform(world, event_loop);
    }

    fn on_window_event(&self, world: &mut World, window: &Window, event: &WindowEvent) -> bool {
        capy_ui::handle_window_event(world, window, event)
    }

    fn on_begin_frame(&self, world: &mut World, window: &Window) {
        capy_ui::begin_frame(world, window);
    }

    fn on_end_frame(&self, world: &mut World, window: &Window) {
        capy_ui::end_frame(world, window);
    }

    fn wants_pointer_input(&self, world: &World) -> bool {
        capy_ui::wants_pointer_input(world)
    }
}
