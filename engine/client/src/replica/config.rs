use crate::replica::{ReplicaError, Result};

#[derive(Clone, Debug)]
pub struct ReplicaConfig {
    pub update_interval: std::time::Duration,
    pub visuals: VisualSettings,
    pub material_colors:
        std::collections::BTreeMap<capy_engine_protocol::world::MaterialId, [f32; 3]>,
    pub render_leaf_edge: i32,
    pub max_mesh_vertices: usize,
    pub max_query_steps: usize,
}

impl Default for ReplicaConfig {
    fn default() -> Self {
        Self {
            update_interval: std::time::Duration::from_millis(50),
            visuals: VisualSettings::default(),
            material_colors: Default::default(),
            render_leaf_edge: 16,
            max_mesh_vertices: 1_000_000,
            max_query_steps: 65_536,
        }
    }
}

#[derive(Clone, Debug)]
pub struct VisualSettings {
    pub ambient_occlusion: bool,
    pub material_variation: bool,
    pub sky: [f32; 3],
    pub sunlight: [f32; 3],
    pub ambient: f32,
    pub fog_distance: f32,
    pub view_distance: f32,
    pub shadows: Option<ShadowSettings>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShadowSettings {
    pub resolution: u32,
    pub distance: f32,
}

impl Default for ShadowSettings {
    fn default() -> Self {
        Self {
            resolution: 2048,
            distance: 96.0,
        }
    }
}

impl VisualSettings {
    pub fn validate(&self) -> Result<()> {
        if !(0.0..=1.0).contains(&self.ambient) {
            return Err(ReplicaError::InvalidSetting(
                "ambient must be between 0 and 1",
            ));
        }

        if !self.fog_distance.is_finite() || self.fog_distance <= 0.0 {
            return Err(ReplicaError::InvalidSetting(
                "fog_distance must be finite and positive",
            ));
        }

        if !self.view_distance.is_finite() || self.view_distance <= 1.0 {
            return Err(ReplicaError::InvalidSetting(
                "view_distance must be finite and greater than 1",
            ));
        }

        if self
            .sky
            .iter()
            .any(|channel| !(0.0..=1.0).contains(channel))
        {
            return Err(ReplicaError::InvalidSetting(
                "sky channels must be between 0 and 1",
            ));
        }

        if let Some(shadows) = self.shadows
            && (!(256..=4096).contains(&shadows.resolution)
                || !shadows.resolution.is_power_of_two()
                || !shadows.distance.is_finite()
                || shadows.distance <= 1.0)
        {
            return Err(ReplicaError::InvalidSetting(
                "shadows require a power-of-two resolution between 256 and 4096 and finite distance greater than 1",
            ));
        }

        let sunlight = glam::Vec3::from_array(self.sunlight);

        if sunlight.try_normalize().is_none() || sunlight.length_squared() < f32::EPSILON {
            return Err(ReplicaError::InvalidSetting(
                "sunlight must be finite with squared length at least f32::EPSILON",
            ));
        }

        Ok(())
    }
}

impl Default for VisualSettings {
    fn default() -> Self {
        Self {
            ambient_occlusion: true,
            material_variation: true,
            sky: [0.48, 0.68, 0.83],
            sunlight: [0.4, 0.85, 0.3],
            ambient: 0.4,
            fog_distance: 100.0,
            view_distance: 160.0,
            shadows: Some(ShadowSettings::default()),
        }
    }
}
