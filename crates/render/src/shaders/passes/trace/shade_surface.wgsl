fn shade_preview(pixel: vec2<i32>, preview_hit: HitResult) -> ShadowCounts {
    let tint_color = vec3<f32>(preview.tint_r, preview.tint_g, preview.tint_b);
    var base = render_settings.material_colors[min(preview_hit.material & 0x7FFFu, 1023u)].rgb;
    base = mix(base, tint_color, preview.tint_strength);

    let shading_pos = preview_hit.hit_pos_local;
    let shading_normal = preview_hit.normal;

    // Preview is fully lit (shadow = 1.0)
    let md = compute_motion_depth(shading_pos);
    write_gbuffer(pixel, base, 1.0, shading_normal, 1.0, preview_hit.t, md);
    return ShadowCounts(0u, 0u);
}

fn shade_grass(pixel: vec2<i32>) -> ShadowCounts {
    let grass = dda_grass_hit;
    if ENABLE_TRACE_STATS { trace_stats_grass_visible_pixels += 1u; }
    let base = grass.color;
    let shading_pos = grass.pos;
    let shading_normal = grass.normal;

    var shadow = 1.0;
    var shadow_ray_count = 0u;
    var shadow_blocked_count = 0u;
    let grass_shadows_enabled =
        FEATURE_GRASS_SHADOWS
        && render_settings.vegetation_shadow_distance > 0.0
        && grass.t <= render_settings.vegetation_shadow_distance;
    if render_settings.sun_contribution > 0.0 && grass_shadows_enabled {
        let sun_dir = normalize(render_settings.sun_direction.xyz);
        let shadow_origin = shading_pos + shading_normal * render_settings.ray_epsilon;
        let in_shadow = trace_shadow_ray(shadow_origin, sun_dir);
        if ENABLE_TRACE_STATS { trace_stats_grass_shadow_rays += 1u; }
        shadow_ray_count = 1u;
        shadow_blocked_count = select(0u, 1u, in_shadow);
        shadow = select(1.0, 0.0, in_shadow);
    }

    let tinted = apply_selection_tint(base, shading_pos);
    let md = compute_motion_depth(shading_pos);
    write_gbuffer(pixel, tinted, shadow, shading_normal, 1.0, grass.t, md);
    return ShadowCounts(shadow_ray_count, shadow_blocked_count);
}

fn shade_voxel(pixel: vec2<i32>, hit: HitResult) -> ShadowCounts {
    let base = render_settings.material_colors[min(hit.material & 0x7FFFu, 1023u)].rgb;

    let shading_pos = hit.hit_pos_local;
    let shading_normal = hit.normal;

    var shadow = 1.0;
    var shadow_ray_count = 0u;
    var shadow_blocked_count = 0u;
    if FEATURE_SHADOWS && render_settings.sun_contribution > 0.0 {
        let sun_dir = normalize(render_settings.sun_direction.xyz);
        let shadow_origin = shading_pos + shading_normal * render_settings.ray_epsilon;
        let in_shadow = trace_shadow_ray(shadow_origin, sun_dir);
        shadow_ray_count = 1u;
        shadow_blocked_count = select(0u, 1u, in_shadow);
        shadow = select(1.0, 0.0, in_shadow);
    }

    let tinted = apply_selection_tint(base, shading_pos);
    let md = compute_motion_depth(shading_pos);
    write_gbuffer(pixel, tinted, shadow, shading_normal, 1.0, hit.t, md);
    return ShadowCounts(shadow_ray_count, shadow_blocked_count);
}
