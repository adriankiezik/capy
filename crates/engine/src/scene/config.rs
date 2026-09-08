use crate::{
    scene::{Result, SceneError},
    world::WorldSettings,
};

#[derive(Clone, Debug)]
pub struct SceneSettings {
    pub world: WorldSettings,
    pub visuals: VisualSettings,
    pub simulation: SimulationSettings,
    pub render_leaf_edge: i32,
    pub max_mesh_vertices: usize,
    pub max_bodies: usize,
}

impl SceneSettings {
    pub fn validate(&self) -> Result<()> {
        if ![8, 16, 32].contains(&self.render_leaf_edge) {
            return Err(SceneError::InvalidSetting(
                "render_leaf_edge must be 8, 16, or 32",
            ));
        }

        if !(1..=u32::MAX as usize).contains(&self.max_mesh_vertices) {
            return Err(SceneError::InvalidSetting(
                "max_mesh_vertices must be between 1 and u32::MAX",
            ));
        }

        if self.max_bodies == 0 {
            return Err(SceneError::InvalidSetting("max_bodies must be positive"));
        }

        self.simulation.validate()?;

        self.visuals.validate()
    }
}

#[derive(Clone, Debug)]
pub struct VisualSettings {
    pub sky: [f32; 3],
    pub sunlight: [f32; 3],
    pub ambient: f32,
    pub fog_distance: f32,
    pub view_distance: f32,
}

impl VisualSettings {
    pub fn validate(&self) -> Result<()> {
        if !(0.0..=1.0).contains(&self.ambient) {
            return Err(SceneError::InvalidSetting(
                "ambient must be between 0 and 1",
            ));
        }

        if !self.fog_distance.is_finite() || self.fog_distance <= 0.0 {
            return Err(SceneError::InvalidSetting(
                "fog_distance must be finite and positive",
            ));
        }

        if !self.view_distance.is_finite() || self.view_distance <= 1.0 {
            return Err(SceneError::InvalidSetting(
                "view_distance must be finite and greater than 1",
            ));
        }

        if self
            .sky
            .iter()
            .any(|channel| !(0.0..=1.0).contains(channel))
        {
            return Err(SceneError::InvalidSetting(
                "sky channels must be between 0 and 1",
            ));
        }

        let sunlight = glam::Vec3::from_array(self.sunlight);

        if !sunlight.is_finite() || sunlight.length_squared() < f32::EPSILON {
            return Err(SceneError::InvalidSetting(
                "sunlight must be finite with squared length at least f32::EPSILON",
            ));
        }

        Ok(())
    }
}

impl Default for VisualSettings {
    fn default() -> Self {
        Self {
            sky: [0.48, 0.68, 0.83],
            sunlight: [0.4, 0.85, 0.3],
            ambient: 0.4,
            fog_distance: 100.0,
            view_distance: 160.0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct SimulationSettings {
    pub gravity: f32,
    pub max_collision_probes: usize,
    pub contact_slop: f32,
}

impl SimulationSettings {
    pub fn validate(&self) -> Result<()> {
        if !self.gravity.is_finite() || self.gravity <= 0.0 {
            return Err(SceneError::InvalidSetting(
                "gravity must be finite and positive",
            ));
        }

        if self.max_collision_probes == 0 {
            return Err(SceneError::InvalidSetting(
                "max_collision_probes must be positive",
            ));
        }

        if !(0.0..0.05).contains(&self.contact_slop) {
            return Err(SceneError::InvalidSetting(
                "contact_slop must be at least 0 and less than 0.05",
            ));
        }

        Ok(())
    }
}

impl Default for SimulationSettings {
    fn default() -> Self {
        Self {
            gravity: 9.81,
            max_collision_probes: 1_000_000,
            contact_slop: 0.004,
        }
    }
}
