
struct CameraUniform {
    inv_view_proj: mat4x4<f32>,
    camera_pos: vec3<f32>,
    _pad0: f32,
    resolution: vec2<f32>,
    lod_bias: f32,
    pixel_size: f32,
    ray_corner: vec3<f32>,
    _pad2: f32,
    ray_right: vec3<f32>,
    _pad3: f32,
    ray_up: vec3<f32>,
    _pad4: f32,
    jitter: vec2<f32>,
    _pad5: vec2<f32>,
    clip_from_world: mat4x4<f32>,
    prev_clip_from_world: mat4x4<f32>,
};
