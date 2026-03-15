use std::error::Error;

use bevy_ecs::prelude::Resource;
use bevy_ecs::world::World;

#[derive(Resource)]
pub struct Runner(
    pub Box<dyn FnOnce(World) -> Result<(), Box<dyn Error + Send + Sync>> + Send + Sync>,
);
