@group(0) @binding(6) var beam_t_out: texture_storage_2d<r32float, write>;

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let dims = textureDimensions(beam_t_out);
    if gid.x >= dims.x || gid.y >= dims.y {
        return;
    }

    // One ray through the center of each 8x8 pixel tile of the trace target.
    let px = f32(gid.x) * 8.0 + 4.0;
    let py = f32(gid.y) * 8.0 + 4.0;
    let uv_x = px / camera.resolution.x;
    let uv_y = 1.0 - py / camera.resolution.y;

    let ray_dir = normalize(
        camera.ray_corner
        + camera.ray_right * (uv_x * 2.0)
        + camera.ray_up * (uv_y * 2.0)
    );

    let t = trace_beam(camera.camera_pos, ray_dir);
    textureStore(beam_t_out, vec2<i32>(i32(gid.x), i32(gid.y)), vec4<f32>(t, 0.0, 0.0, 0.0));
}
