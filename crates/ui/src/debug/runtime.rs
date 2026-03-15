use bevy_ecs::world::World;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::Window as WinitWindow;

use super::{EguiContext, EguiPlatformState, EguiRenderOutput, UiEnabled};

pub fn initialize_platform(world: &mut World, event_loop: &ActiveEventLoop) {
    if world.get_resource::<UiEnabled>().is_none()
        || world.get_non_send_resource::<EguiPlatformState>().is_some()
    {
        return;
    }

    let Some(egui_context) = world.get_resource::<EguiContext>().cloned() else {
        return;
    };

    let state = egui_winit::State::new(
        egui_context.0.clone(),
        egui::ViewportId::ROOT,
        event_loop,
        None,
        None,
        None,
    );
    world.insert_non_send_resource(EguiPlatformState { state });
}

pub fn handle_window_event(world: &mut World, window: &WinitWindow, event: &WindowEvent) -> bool {
    if world.get_resource::<UiEnabled>().is_none() {
        return false;
    }

    let Some(mut platform_state) = world.get_non_send_resource_mut::<EguiPlatformState>() else {
        return false;
    };

    platform_state.state.on_window_event(window, event).consumed
}

pub fn wants_pointer_input(world: &World) -> bool {
    if world.get_resource::<UiEnabled>().is_none() {
        return false;
    }

    world
        .get_resource::<EguiContext>()
        .map(|ctx| ctx.0.wants_pointer_input())
        .unwrap_or(false)
}

pub fn begin_frame(world: &mut World, window: &WinitWindow) {
    if world.get_resource::<UiEnabled>().is_none() {
        return;
    }

    let Some(egui_context) = world.get_resource::<EguiContext>().cloned() else {
        return;
    };
    let Some(mut platform_state) = world.get_non_send_resource_mut::<EguiPlatformState>() else {
        return;
    };

    let raw_input = platform_state.state.take_egui_input(window);
    egui_context.0.begin_pass(raw_input);
}

pub fn end_frame(world: &mut World, window: &WinitWindow) {
    if world.get_resource::<UiEnabled>().is_none() {
        return;
    }

    let Some(egui_context) = world.get_resource::<EguiContext>().cloned() else {
        return;
    };
    let full_output = egui_context.0.end_pass();
    {
        let Some(mut platform_state) = world.get_non_send_resource_mut::<EguiPlatformState>()
        else {
            return;
        };
        platform_state
            .state
            .handle_platform_output(window, full_output.platform_output);
    }

    let pixels_per_point = egui_context.0.pixels_per_point();
    let clipped_primitives = egui_context
        .0
        .tessellate(full_output.shapes, pixels_per_point);
    let size = window.inner_size();
    let render_output = EguiRenderOutput {
        clipped_primitives,
        textures_delta: full_output.textures_delta,
        pixels_per_point,
        screen_size: [size.width, size.height],
    };
    world.insert_non_send_resource(render_output);
}

pub fn render_output(world: &mut World) -> Option<EguiRenderOutput> {
    world.remove_non_send_resource::<EguiRenderOutput>()
}
