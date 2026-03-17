use bevy_ecs::resource::Resource;

pub const MATERIAL_PALETTE_SIZE: usize = capy_core::MATERIAL_PALETTE_SIZE;

#[derive(Resource, Clone, Debug)]
pub struct RendererSettings {
    pub sun_direction: [f32; 3],
    pub ambient_light: f32,
    pub sun_contribution: f32,
    pub sky_color: [f32; 3],
    pub material_palette: [[f32; 3]; MATERIAL_PALETTE_SIZE],

    pub ray_epsilon: f32,
    pub max_chunk_steps: u32,
    pub max_node_steps: u32,

    pub chunk_lod_scale: f32,
    pub node_lod_scale: f32,
    pub lod_bias: f32,

    pub ao_radius: f32,
    pub ao_intensity: f32,
    pub ao_samples: u32,
    pub ao_steps: u32,
}

impl RendererSettings {
    pub fn with_palette(material_palette: [[f32; 3]; MATERIAL_PALETTE_SIZE]) -> Self {
        Self {
            material_palette,
            ..Self::default()
        }
    }
}

impl Default for RendererSettings {
    fn default() -> Self {
        Self {
            sun_direction: [0.5, 1.0, 0.3],
            ambient_light: 0.2,
            sun_contribution: 0.8,
            sky_color: [0.4, 0.6, 0.9],
            material_palette: capy_core::MATERIAL_COLORS,
            ray_epsilon: 0.0001,
            max_chunk_steps: 64,
            max_node_steps: 512,
            chunk_lod_scale: 0.25,
            node_lod_scale: 1.0,
            lod_bias: 1.0,
            ao_radius: 2.0,
            ao_intensity: 1.0,
            ao_samples: 4,
            ao_steps: 4,
        }
    }
}
