fn write_primary_voxel(pixel: vec2<i32>, hit: HitResult) {
    let base = render_settings.material_colors[min(hit.material & 0x7FFFu, 1023u)].rgb;

    let md = compute_motion_depth(hit.hit_pos_local);
    write_gbuffer(pixel, base, 1.0, hit.normal, 1.0, hit.t, md);
}

fn write_primary_sky(pixel: vec2<i32>, ray_dir: vec3<f32>) {
    let sky_pos = camera.camera_pos + ray_dir * 100000.0;
    let sky_md_raw = compute_motion_depth(sky_pos);
    let sky_md = MotionDepth(sky_md_raw.motion, 1.0);
    write_gbuffer(pixel, vec3<f32>(0.0), 0.0, vec3<f32>(0.0), -1.0, 0.0, sky_md);
}

@compute @workgroup_size(8, 4)
fn main(
    @builtin(global_invocation_id) gid: vec3<u32>,
    @builtin(local_invocation_index) lid: u32,
) {
    let dims = textureDimensions(gbuf_color_out);
    let actual_x = gid.x;
    let actual_y = gid.y;
    let in_bounds = actual_x < dims.x && actual_y < dims.y;
    reset_primary_only_trace_stats();

    var hit: HitResult;

    if in_bounds {
        let uv_x = (f32(actual_x) + 0.5 + camera.jitter.x) / camera.resolution.x;
        let uv_y = 1.0 - (f32(actual_y) + 0.5 + camera.jitter.y) / camera.resolution.y;

        let ray_dir = normalize(
            camera.ray_corner
            + camera.ray_right * (uv_x * 2.0)
            + camera.ray_up * (uv_y * 2.0)
        );
        let ray_origin = camera.camera_pos;
        let t_start = beam_start_t(actual_x, actual_y);
        hit = trace_ray_primary_only(ray_origin, ray_dir, t_start);
        let pixel = vec2<i32>(i32(actual_x), i32(actual_y));

        if hit.hit {
            write_primary_voxel(pixel, hit);
        } else {
            write_primary_sky(pixel, ray_dir);
        }
    }

    commit_primary_only_trace_stats(lid, in_bounds, hit);
}
