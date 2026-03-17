struct RenderSettingsUniform {
    sun_direction: vec4<f32>,
    sky_color: vec4<f32>,
    material_colors: array<vec4<f32>, 1024>,

    ambient_light: f32,
    sun_contribution: f32,
    chunk_lod_scale: f32,
    node_lod_scale: f32,

    ray_epsilon: f32,
    max_chunk_steps: f32,
    max_node_steps: f32,
    _pad0: f32,
};
