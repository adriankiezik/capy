use std::path::{Path, PathBuf};

use bevy_ecs::resource::Resource;

#[derive(Resource)]
pub struct WorldHandle {
    world_dir: PathBuf,
}

impl WorldHandle {
    pub fn new(world_dir: PathBuf) -> Self {
        Self { world_dir }
    }

    pub fn world_dir(&self) -> &Path {
        &self.world_dir
    }
}
