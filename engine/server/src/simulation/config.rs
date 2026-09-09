use crate::{
    simulation::{Result, SimulationError},
    world::WorldSettings,
};

#[derive(Clone, Debug)]
pub struct SimulationConfig {
    pub world: WorldSettings,
    pub physics: PhysicsConfig,
    pub max_bodies: usize,
}

impl SimulationConfig {
    pub fn validate(&self) -> Result<()> {
        if self.max_bodies == 0 {
            return Err(SimulationError::InvalidSetting(
                "max_bodies must be positive",
            ));
        }

        self.physics.validate()
    }
}

#[derive(Clone, Debug)]
pub struct PhysicsConfig {
    pub gravity: f32,
    pub max_collision_probes: usize,
    pub contact_slop: f32,
}

impl PhysicsConfig {
    pub fn validate(&self) -> Result<()> {
        if !self.gravity.is_finite() || self.gravity <= 0.0 {
            return Err(SimulationError::InvalidSetting(
                "gravity must be finite and positive",
            ));
        }

        if self.max_collision_probes == 0 {
            return Err(SimulationError::InvalidSetting(
                "max_collision_probes must be positive",
            ));
        }

        if !(0.0..0.05).contains(&self.contact_slop) {
            return Err(SimulationError::InvalidSetting(
                "contact_slop must be at least 0 and less than 0.05",
            ));
        }

        Ok(())
    }
}

impl Default for PhysicsConfig {
    fn default() -> Self {
        Self {
            gravity: 9.81,
            max_collision_probes: 1_000_000,
            contact_slop: 0.004,
        }
    }
}
