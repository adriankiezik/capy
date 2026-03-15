use bevy_ecs::prelude::Resource;

#[derive(Resource)]
pub struct TickRate {
    pub ticks_per_second: u32,
}

impl Default for TickRate {
    fn default() -> Self {
        Self {
            ticks_per_second: 20,
        }
    }
}
