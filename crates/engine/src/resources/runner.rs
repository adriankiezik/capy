use std::error::Error;

use bevy_ecs::prelude::Resource;
use bevy_ecs::world::World;

type RunnerFn = Box<dyn FnOnce(World) -> Result<(), Box<dyn Error + Send + Sync>> + Send + Sync>;

#[derive(Resource)]
pub struct Runner(pub RunnerFn);
