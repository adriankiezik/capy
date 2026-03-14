use bytemuck::{Pod, Zeroable};

pub(crate) const MATERIAL_PALETTE_SIZE: usize = 8;

#[derive(Clone, Debug)]
pub(crate) struct RendererSettings {
    pub(crate) sun_direction: [f32; 3],
    pub(crate) ambient_light: f32,
    pub(crate) sun_contribution: f32,
    pub(crate) sky_color: [f32; 3],
    pub(crate) material_palette: [[f32; 3]; MATERIAL_PALETTE_SIZE],

    pub(crate) near_blend_multiplier: f32,
    pub(crate) near_blend_min: f32,
    pub(crate) far_blend_multiplier: f32,
    pub(crate) far_blend_min: f32,
    pub(crate) depth_transition_start: f32,
    pub(crate) depth_transition_end: f32,
    pub(crate) motion_threshold_divisor: f32,
    pub(crate) max_blend_cap: f32,

    pub(crate) angular_error_multiplier: f32,
    pub(crate) depth_diff_threshold: f32,
    pub(crate) neighbor_interpolation_epsilon: f32,

    pub(crate) ray_epsilon: f32,
    pub(crate) max_chunk_steps: u32,
    pub(crate) max_node_steps: u32,

    pub(crate) chunk_lod_scale: f32,
    pub(crate) node_lod_scale: f32,
}

impl RendererSettings {
    pub(crate) fn with_palette(material_palette: [[f32; 3]; MATERIAL_PALETTE_SIZE]) -> Self {
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
            material_palette: [
                [0.0, 0.0, 0.0],
                [0.8, 0.2, 0.2],
                [0.2, 0.8, 0.2],
                [0.2, 0.2, 0.8],
                [0.8, 0.8, 0.2],
                [0.8, 0.2, 0.8],
                [0.2, 0.8, 0.8],
                [0.8, 0.8, 0.8],
            ],
            near_blend_multiplier: 3.0,
            near_blend_min: 0.16,
            far_blend_multiplier: 0.6,
            far_blend_min: 0.02,
            depth_transition_start: 12.0,
            depth_transition_end: 108.0,
            motion_threshold_divisor: 1.25,
            max_blend_cap: 0.35,
            angular_error_multiplier: 3.0,
            depth_diff_threshold: 0.15,
            neighbor_interpolation_epsilon: 0.02,
            ray_epsilon: 0.0001,
            max_chunk_steps: 64,
            max_node_steps: 512,
            chunk_lod_scale: 0.25,
            node_lod_scale: 1.0,
        }
    }
}

impl RendererSettings {
    pub(crate) fn to_uniform(&self) -> RenderSettingsUniform {
        let mut material_colors = [[0.0f32; 4]; MATERIAL_PALETTE_SIZE];
        for (dst, src) in material_colors.iter_mut().zip(self.material_palette.iter()) {
            dst[0] = src[0];
            dst[1] = src[1];
            dst[2] = src[2];
            dst[3] = 1.0;
        }

        let depth_transition_end = self
            .depth_transition_end
            .max(self.depth_transition_start + 0.0001);
        RenderSettingsUniform {
            sun_direction: [
                self.sun_direction[0],
                self.sun_direction[1],
                self.sun_direction[2],
                0.0,
            ],
            sky_color: [self.sky_color[0], self.sky_color[1], self.sky_color[2], 0.0],
            material_colors,
            ambient_light: self.ambient_light.max(0.0),
            sun_contribution: self.sun_contribution.max(0.0),
            chunk_lod_scale: self.chunk_lod_scale.max(0.0),
            node_lod_scale: self.node_lod_scale.max(0.0),
            near_blend_multiplier: self.near_blend_multiplier.max(0.0),
            near_blend_min: self.near_blend_min.max(0.0),
            far_blend_multiplier: self.far_blend_multiplier.max(0.0),
            far_blend_min: self.far_blend_min.max(0.0),
            depth_transition_start: self.depth_transition_start.max(0.0),
            depth_transition_end,
            motion_threshold_divisor: self.motion_threshold_divisor.max(0.0001),
            max_blend_cap: self.max_blend_cap.max(0.0),
            angular_error_multiplier: self.angular_error_multiplier.max(0.0),
            depth_diff_threshold: self.depth_diff_threshold.max(0.0),
            neighbor_interpolation_epsilon: self.neighbor_interpolation_epsilon.max(0.0),
            ray_epsilon: self.ray_epsilon.max(0.0),
            max_chunk_steps: self.max_chunk_steps.max(1) as f32,
            max_node_steps: self.max_node_steps.max(1) as f32,
            enable_adaptive_blend: 1.0,
            _pad0: [0.0; 5],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct RenderSettingsUniform {
    sun_direction: [f32; 4],
    sky_color: [f32; 4],
    material_colors: [[f32; 4]; MATERIAL_PALETTE_SIZE],

    ambient_light: f32,
    sun_contribution: f32,
    chunk_lod_scale: f32,
    node_lod_scale: f32,

    near_blend_multiplier: f32,
    near_blend_min: f32,
    far_blend_multiplier: f32,
    far_blend_min: f32,

    depth_transition_start: f32,
    depth_transition_end: f32,
    motion_threshold_divisor: f32,
    max_blend_cap: f32,

    angular_error_multiplier: f32,
    depth_diff_threshold: f32,
    neighbor_interpolation_epsilon: f32,
    ray_epsilon: f32,

    max_chunk_steps: f32,
    max_node_steps: f32,
    enable_adaptive_blend: f32,
    _pad0: [f32; 5],
}

const _: () = assert!(std::mem::size_of::<RenderSettingsUniform>() == 256);

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct StreamingInfoUniform {
    pub(crate) grid_min_x: i32,
    pub(crate) grid_min_y: i32,
    pub(crate) grid_min_z: i32,
    pub(crate) grid_dim: u32,
    pub(crate) chunk_size: u32,
    pub(crate) pool_slot_count: u32,
    pub(crate) _pad0: u32,
    pub(crate) _pad1: u32,
}

const _: () = assert!(std::mem::size_of::<StreamingInfoUniform>() == 32);
