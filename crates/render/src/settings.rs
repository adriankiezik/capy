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

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct PreviewParamsUniform {
    pub(crate) is_active: u32,
    pub(crate) pool_offset: u32,
    pub(crate) world_size: u32,
    pub(crate) root_offset: u32,
    pub(crate) depth: u32,
    pub(crate) _pad0: u32,
    pub(crate) _pad1: u32,
    pub(crate) _pad2: u32,
    pub(crate) pos_x: f32,
    pub(crate) pos_y: f32,
    pub(crate) pos_z: f32,
    pub(crate) tint_strength: f32,
    pub(crate) tint_r: f32,
    pub(crate) tint_g: f32,
    pub(crate) tint_b: f32,
    pub(crate) _pad3: f32,
}

const _: () = assert!(std::mem::size_of::<PreviewParamsUniform>() == 64);

impl PreviewParamsUniform {
    pub(crate) fn inactive() -> Self {
        Self {
            is_active: 0,
            pool_offset: 0,
            world_size: 0,
            root_offset: 0,
            depth: 0,
            _pad0: 0,
            _pad1: 0,
            _pad2: 0,
            pos_x: 0.0,
            pos_y: 0.0,
            pos_z: 0.0,
            tint_strength: 0.0,
            tint_r: 0.0,
            tint_g: 0.0,
            tint_b: 0.0,
            _pad3: 0.0,
        }
    }

    pub(crate) fn from_gpu_data(data: &capy_core::PreviewGpuData) -> Self {
        Self {
            is_active: u32::from(data.active),
            pool_offset: data.pool_offset,
            world_size: data.world_size,
            root_offset: data.root_offset,
            depth: data.depth,
            _pad0: 0,
            _pad1: 0,
            _pad2: 0,
            pos_x: data.position[0],
            pos_y: data.position[1],
            pos_z: data.position[2],
            tint_strength: data.tint_strength,
            tint_r: data.tint[0],
            tint_g: data.tint[1],
            tint_b: data.tint[2],
            _pad3: 0.0,
        }
    }
}
