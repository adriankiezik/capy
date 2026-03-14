struct RenderSettingsUniform {
    sun_direction: vec4<f32>,
    sky_color: vec4<f32>,
    material_colors: array<vec4<f32>, 8>,

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
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
    _pad3: f32,
    _pad4: f32,
};
