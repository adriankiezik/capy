struct ShadowCounts {
    rays: u32,
    blocked: u32,
};

struct MotionDepth {
    motion: vec2<f32>,
    hardware_depth: f32,
};

fn compute_motion_depth(world_pos: vec3<f32>) -> MotionDepth {
    let clip_pos = camera.clip_from_world * vec4<f32>(world_pos, 1.0);
    let prev_clip_pos = camera.prev_clip_from_world * vec4<f32>(world_pos, 1.0);
    let curr_ndc = clip_pos.xy / clip_pos.w;
    let prev_ndc = prev_clip_pos.xy / prev_clip_pos.w;
    let motion = (curr_ndc - prev_ndc) * vec2<f32>(0.5, -0.5);
    let hardware_depth = clamp(clip_pos.z / clip_pos.w, 0.0, 1.0);
    return MotionDepth(motion, hardware_depth);
}

fn write_gbuffer(
    pixel: vec2<i32>,
    color: vec3<f32>,
    shadow: f32,
    normal: vec3<f32>,
    normal_w: f32,
    ray_depth: f32,
    md: MotionDepth,
) {
    textureStore(gbuf_color_out, pixel, vec4<f32>(color, shadow));
    textureStore(gbuf_normal_out, pixel, vec4<f32>(normal, normal_w));
    textureStore(gbuf_depth_out, pixel, vec4<f32>(ray_depth, 0.0, 0.0, 0.0));
    textureStore(dlss_depth_out, pixel, vec4<f32>(md.hardware_depth, 0.0, 0.0, 0.0));
    textureStore(motion_vectors_out, pixel, vec4<f32>(md.motion, 0.0, 0.0));
}
