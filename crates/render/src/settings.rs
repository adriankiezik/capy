use bytemuck::{Pod, Zeroable};

use crate::resources::{MATERIAL_PALETTE_SIZE, RendererSettings};

pub(crate) fn to_render_settings_uniform(settings: &RendererSettings) -> RenderSettingsUniform {
    let mut material_colors = [[0.0f32; 4]; MATERIAL_PALETTE_SIZE];
    for (dst, src) in material_colors
        .iter_mut()
        .zip(settings.material_palette.iter())
    {
        dst[0] = src[0];
        dst[1] = src[1];
        dst[2] = src[2];
        dst[3] = 1.0;
    }

    RenderSettingsUniform {
        sun_direction: [
            settings.sun_direction[0],
            settings.sun_direction[1],
            settings.sun_direction[2],
            0.0,
        ],
        sky_color: [
            settings.sky_color[0],
            settings.sky_color[1],
            settings.sky_color[2],
            0.0,
        ],
        material_colors,
        ambient_light: settings.ambient_light.max(0.0),
        sun_contribution: settings.sun_contribution.max(0.0),
        chunk_lod_scale: settings.chunk_lod_scale.max(0.0),
        node_lod_scale: settings.node_lod_scale.max(0.0),
        ray_epsilon: settings.ray_epsilon.max(0.0),
        max_chunk_steps: settings.max_chunk_steps.max(1) as f32,
        max_node_steps: settings.max_node_steps.max(1) as f32,
        _padding0: 0.0,
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

    ray_epsilon: f32,
    max_chunk_steps: f32,
    max_node_steps: f32,
    _padding0: f32,
}

const _: () = assert!(std::mem::size_of::<RenderSettingsUniform>() == 16448);

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct StreamingInfoUniform {
    pub(crate) grid_min_x: i32,
    pub(crate) grid_min_y: i32,
    pub(crate) grid_min_z: i32,
    pub(crate) grid_dim_x: u32,
    pub(crate) grid_dim_y: u32,
    pub(crate) grid_dim_z: u32,
    pub(crate) chunk_size_xz: u32,
    pub(crate) chunk_size_y: u32,
}

const _: () = assert!(std::mem::size_of::<StreamingInfoUniform>() == 32);
