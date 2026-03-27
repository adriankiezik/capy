@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let dims = textureDimensions(gbuf_color_out);

    let actual_x = gid.x;
    let actual_y = gid.y;
    if actual_x >= dims.x || actual_y >= dims.y { return; }

    reset_trace_private_stats();
    skip_grass = render_settings.vegetation_enabled < 0.5;

    let uv_x = (f32(actual_x) + 0.5 + camera.jitter.x) / camera.resolution.x;
    let uv_y = 1.0 - (f32(actual_y) + 0.5 + camera.jitter.y) / camera.resolution.y;

    let ray_dir = normalize(
        camera.ray_corner
        + camera.ray_right * (uv_x * 2.0)
        + camera.ray_up * (uv_y * 2.0)
    );
    let ray_origin = camera.camera_pos;

    // Trace voxels + grass integrated in the DDA loop
    let hit = trace_ray(ray_origin, ray_dir);
    let sel = resolve_visible_hit(ray_origin, ray_dir, hit);

    let pixel_idx = actual_y * u32(camera.resolution.x) + actual_x;
    if pixel_idx < arrayLength(&lod_debug_buf) {
        lod_debug_buf[pixel_idx] = hit.lod_scale_exp;
    }

    let pixel = vec2<i32>(i32(actual_x), i32(actual_y));
    var counts: ShadowCounts;

    if sel.use_water {
        counts = shade_water(pixel, ray_origin, ray_dir, hit);
    } else if sel.use_preview {
        counts = shade_preview(pixel, sel.preview_hit);
    } else if sel.use_grass {
        counts = shade_grass(pixel);
    } else if hit.hit {
        counts = shade_voxel(pixel, hit);
    } else {
        counts = shade_sky(pixel, ray_origin, ray_dir);
    }

    commit_trace_stats(hit, counts.rays, counts.blocked);
}
