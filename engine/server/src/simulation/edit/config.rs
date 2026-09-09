use crate::simulation::{Result, Simulation, SimulationError};

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
            return Err(SimulationError::InvalidSetting(
                "edits require 1..=64 workers and 0 < max_batch <= max_pending",
            ));
        }

        Ok(())
    }
}

#[derive(Clone, Copy)]
pub(in crate::simulation) struct Limits {
    pub(super) edge: i32,
    pub(in crate::simulation) bodies: usize,
}

impl Limits {
    pub(in crate::simulation) fn new(scene: &Simulation) -> Self {
        Self {
            edge: crate::world::LEAF_EDGE,
            bodies: scene.settings.max_bodies,
        }
    }
}
