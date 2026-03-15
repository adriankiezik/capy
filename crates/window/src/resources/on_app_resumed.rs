use bevy_ecs::prelude::Resource;
use bevy_ecs::world::World;
use winit::event_loop::ActiveEventLoop;

#[derive(Resource, Default)]
pub struct OnAppResumed {
    callbacks: Vec<fn(&mut World, &ActiveEventLoop)>,
}

impl OnAppResumed {
    pub fn add(&mut self, callback: fn(&mut World, &ActiveEventLoop)) {
        self.callbacks.push(callback);
    }

    pub(crate) fn invoke(&self, world: &mut World, event_loop: &ActiveEventLoop) {
        for callback in &self.callbacks {
            callback(world, event_loop);
        }
    }
}
