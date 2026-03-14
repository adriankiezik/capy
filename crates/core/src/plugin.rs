use bevy_ecs::world::World;

pub trait Plugin {
    fn register(&self, world: &mut World);
}
