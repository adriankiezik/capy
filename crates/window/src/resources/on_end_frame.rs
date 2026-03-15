use bevy_ecs::prelude::Resource;
use bevy_ecs::world::World;
use winit::window::Window;

#[derive(Resource, Default)]
pub struct OnEndFrame {
    callbacks: Vec<fn(&mut World, &Window)>,
}

impl OnEndFrame {
    pub fn add(&mut self, callback: fn(&mut World, &Window)) {
        self.callbacks.push(callback);
    }

    pub(crate) fn invoke(&self, world: &mut World, window: &Window) {
        for callback in &self.callbacks {
            callback(world, window);
        }
    }
}
