use bevy_ecs::world::World;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::Window;

pub trait EnginePlugin {
    fn register(&self, world: &mut World);

    fn on_app_resumed(&self, _world: &mut World, _event_loop: &ActiveEventLoop) {}

    fn on_window_event(&self, _world: &mut World, _window: &Window, _event: &WindowEvent) -> bool {
        false
    }

    fn on_begin_frame(&self, _world: &mut World, _window: &Window) {}

    fn on_end_frame(&self, _world: &mut World, _window: &Window) {}

    fn wants_pointer_input(&self, _world: &World) -> bool {
        false
    }
}

pub struct CorePluginAdapter<P: capy_core::Plugin> {
    plugin: P,
}

impl<P: capy_core::Plugin> CorePluginAdapter<P> {
    pub fn new(plugin: P) -> Self {
        Self { plugin }
    }
}

impl<P: capy_core::Plugin> EnginePlugin for CorePluginAdapter<P> {
    fn register(&self, world: &mut World) {
        self.plugin.register(world);
    }
}
