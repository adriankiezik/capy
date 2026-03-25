use bevy_ecs::resource::Resource;

pub const MATERIAL_PALETTE_SIZE: usize = capy_core::MATERIAL_PALETTE_SIZE;
pub const DEFAULT_RENDER_SCALE: f32 = 1.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AoMode {
    #[default]
    ScreenSpace,
    RayTraced,
}

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
    pub ao_mode: AoMode,
    pub ao_rays: u32,

    pub vegetation_enabled: bool,
    pub vegetation_density: f32,
    pub vegetation_max_distance: f32,
    pub vegetation_far_step_scale: f32,
    pub vegetation_far_reduce_start: f32,
    pub vegetation_near_search_radius: u32,
    pub vegetation_far_search_radius: u32,
    pub vegetation_shadow_enabled: bool,
    pub vegetation_shadow_distance: f32,
    pub vegetation_animation_distance: f32,

    pub water_enabled: bool,
    pub water_reflections: bool,
    pub water_reflection_distance: f32,
    pub water_shadows: bool,
    pub water_shadow_distance: f32,

    /// Internal render resolution as a fraction of window size.
    /// 1.0 = native, 0.5 = half resolution, 0.25 = quarter resolution.
    pub render_scale: f32,
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
            ao_mode: AoMode::default(),
            ao_rays: 4,
            vegetation_enabled: true,
            vegetation_density: 1.0,
            vegetation_max_distance: 4000.0,
            vegetation_far_step_scale: 4.0,
            vegetation_far_reduce_start: 500.0,
            vegetation_near_search_radius: 1,
            vegetation_far_search_radius: 0,
            vegetation_shadow_enabled: true,
            vegetation_shadow_distance: 4000.0,
            vegetation_animation_distance: 2000.0,
            water_enabled: true,
            water_reflections: true,
            water_reflection_distance: 300.0,
            water_shadows: true,
            water_shadow_distance: 2000.0,
            render_scale: DEFAULT_RENDER_SCALE,
        }
    }
}

impl RendererSettings {
    /// Compute the internal render resolution from window dimensions and render_scale.
    pub fn scaled_resolution(&self, window_width: u32, window_height: u32) -> (u32, u32) {
        compute_scaled_resolution(window_width, window_height, self.render_scale)
    }
}

/// Compute internal render resolution from window dimensions and a scale factor.
pub(crate) fn compute_scaled_resolution(
    window_width: u32,
    window_height: u32,
    render_scale: f32,
) -> (u32, u32) {
    let scale = render_scale.clamp(0.1, 1.0);
    let w = ((window_width as f32) * scale).round() as u32;
    let h = ((window_height as f32) * scale).round() as u32;
    (w.max(1), h.max(1))
}
