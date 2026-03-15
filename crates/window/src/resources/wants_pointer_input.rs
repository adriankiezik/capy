use bevy_ecs::prelude::Resource;
use bevy_ecs::world::World;

#[derive(Resource, Default)]
pub struct WantsPointerInput {
    callbacks: Vec<fn(&World) -> bool>,
}

impl WantsPointerInput {
    pub fn add(&mut self, callback: fn(&World) -> bool) {
        self.callbacks.push(callback);
    }

    pub(crate) fn invoke(&self, world: &World) -> bool {
        self.callbacks.iter().any(|callback| callback(world))
    }
}
