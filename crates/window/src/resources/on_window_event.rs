use bevy_ecs::prelude::Resource;
use bevy_ecs::world::World;
use winit::event::WindowEvent;
use winit::window::Window;

#[derive(Resource, Default)]
pub struct OnWindowEvent {
    callbacks: Vec<fn(&mut World, &Window, &WindowEvent) -> bool>,
}

impl OnWindowEvent {
    pub fn add(&mut self, callback: fn(&mut World, &Window, &WindowEvent) -> bool) {
        self.callbacks.push(callback);
    }

    pub(crate) fn invoke(&self, world: &mut World, window: &Window, event: &WindowEvent) -> bool {
        let mut consumed = false;
        for callback in &self.callbacks {
            consumed |= callback(world, window, event);
        }
        consumed
    }
}
