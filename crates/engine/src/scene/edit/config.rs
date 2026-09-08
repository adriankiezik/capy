use crate::scene::{Result, Scene, SceneError};

#[derive(Clone, Copy, Debug)]
pub struct EditSettings {
    pub workers: usize,
    pub max_pending: usize,
    pub max_batch: usize,
}

impl Default for EditSettings {
    fn default() -> Self {
        Self {
            workers: std::thread::available_parallelism()
                .map_or(1, |count| count.get().saturating_sub(1).clamp(1, 4)),
            max_pending: 4096,
            max_batch: 256,
        }
    }
}

impl EditSettings {
    pub(super) fn validate(&self) -> Result<()> {
        if !(1..=64).contains(&self.workers)
            || self.max_pending == 0
            || self.max_batch == 0
            || self.max_batch > self.max_pending
        {
            return Err(SceneError::InvalidSetting(
                "edits require 1..=64 workers and 0 < max_batch <= max_pending",
            ));
        }

        Ok(())
    }
}

#[derive(Clone, Copy)]
pub(in crate::scene) struct Limits {
    pub(super) edge: i32,
    pub(in crate::scene) bodies: usize,
    pub(super) vertices: usize,
}

impl Limits {
    pub(in crate::scene) fn new(scene: &Scene) -> Self {
        Self {
            edge: scene.settings.render_leaf_edge,
            bodies: scene.settings.max_bodies,
            vertices: scene.settings.max_mesh_vertices,
        }
    }
}
