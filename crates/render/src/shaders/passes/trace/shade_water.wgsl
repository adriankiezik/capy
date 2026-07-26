const UNDERWATER_SURFACE_DEPTH_MIN: f32 = 2.5;
const UNDERWATER_SURFACE_DEPTH_MAX: f32 = 18.0;

fn underwater_surface_depth(ray_dir: vec3<f32>) -> f32 {
    let up = clamp(ray_dir.y, 0.0, 1.0);
    return mix(UNDERWATER_SURFACE_DEPTH_MAX, UNDERWATER_SURFACE_DEPTH_MIN, sqrt(up));
}

fn underwater_sky_view(ray_origin: vec3<f32>, ray_dir: vec3<f32>) -> vec3<f32> {
    let sun_dir = normalize(render_settings.sun_direction.xyz);
    let surface_t = underwater_surface_depth(ray_dir);
    let surface_pos = ray_origin + ray_dir * surface_t;
    let surface_n = water_normal(surface_pos.xz, camera.time);
    // Sky distortion is handled by the lighting pass screen-space wobble;
    // here we only apply absorption, edge fade, and ripple shading.
    let sky_dir = normalize(vec3<f32>(ray_dir.x, max(ray_dir.y, 0.05), ray_dir.z));
    let transmitted = water_absorb(sky_color(sky_dir, sun_dir), surface_t);
    let edge_fade = pow(1.0 - clamp(ray_dir.y, 0.0, 1.0), 1.5);
    let ripple_shade = mix(0.92, 1.0, surface_n.y);
    return mix(transmitted * ripple_shade, WATER_DEEP_COLOR, edge_fade * 0.18);
}

fn shade_water(pixel: vec2<i32>, ray_origin: vec3<f32>, ray_dir: vec3<f32>, hit: HitResult) -> ShadowCounts {
    let water = dda_water_hit;
    let grass = dda_grass_hit;

    if ENABLE_TRACE_STATS { trace_stats_water_pixels += 1u; }
    let surface_pos = ray_origin + ray_dir * water.t;
    let sun_dir = normalize(render_settings.sun_direction.xyz);

    var water_color: vec3<f32>;
    var water_n: vec3<f32>;
    var shadow = 1.0;

    // Underwater color shared by both top-surface and side-face paths
    var refraction = WATER_DEEP_COLOR;
    let raster_solid_depth = textureLoad(near_mesh_depth, pixel, 0).r;
    let raster_solid_behind = raster_solid_depth > water.t;
    let grass_behind = grass.hit
        && (!hit.hit || grass.t < hit.t)
        && (!raster_solid_behind || grass.t < raster_solid_depth);
    if grass_behind {
        if ENABLE_TRACE_STATS { trace_stats_water_absorb_evals += 1u; }
        let uw_dist = grass.t - water.t;
        refraction = water_absorb(grass.color, uw_dist);
    } else if raster_solid_behind && (!hit.hit || raster_solid_depth < hit.t) {
        if ENABLE_TRACE_STATS { trace_stats_water_absorb_evals += 1u; }
        let uw_dist = raster_solid_depth - water.t;
        let raster_color = textureLoad(near_mesh_color, pixel, 0).rgb;
        refraction = water_absorb(raster_color, uw_dist);
    } else if hit.hit {
        if ENABLE_TRACE_STATS { trace_stats_water_absorb_evals += 1u; }
        let uw_dist = hit.t - water.t;
        var uw_color: vec3<f32>;
        if hit.is_lod_hit {
            uw_color = hit.color_override;
        } else {
            uw_color = render_settings.material_colors[min(hit.material & 0x3FFFu, 1023u)].rgb;
        }
        refraction = water_absorb(uw_color, uw_dist);
    }

    // Track when nothing was found behind the water (deep color fallback)
    if ENABLE_TRACE_STATS {
        let has_solid_behind = grass_behind || raster_solid_behind || hit.hit;
        if !has_solid_behind {
            trace_stats_water_deep_no_hit += 1u;
        }
    }

    let water_is_top_face = water.entry_normal.y > 0.5;
    if water_is_top_face {
        if ENABLE_TRACE_STATS { trace_stats_water_top_face_pixels += 1u; }
        // --- Top-face: full water surface shading (waves, Fresnel, reflections) ---
        let snapped_xz = snap_water_xz(surface_pos.xz);
        let snapped_surface = vec3<f32>(snapped_xz.x, surface_pos.y, snapped_xz.y);

        // Distance-based water normal LOD: fewer fbm octaves at distance
        if ENABLE_TRACE_STATS { trace_stats_water_normal_evals += 1u; }
        var perturbed_n: vec3<f32>;
        if water.t > WATER_NORMAL_FLAT_DIST {
            if ENABLE_TRACE_STATS { trace_stats_water_normal_lod += 1u; }
            perturbed_n = vec3<f32>(0.0, 1.0, 0.0);
        } else if water.t > WATER_NORMAL_LOD2_DIST {
            if ENABLE_TRACE_STATS { trace_stats_water_normal_lod += 1u; }
            perturbed_n = water_normal_lod(snapped_xz, camera.time, 1);
        } else if water.t > WATER_NORMAL_LOD1_DIST {
            if ENABLE_TRACE_STATS { trace_stats_water_normal_lod += 1u; }
            perturbed_n = water_normal_lod(snapped_xz, camera.time, 2);
        } else {
            perturbed_n = water_normal(snapped_xz, camera.time);
        }

        let tile_view_dir = normalize(camera.camera_pos - snapped_surface);
        let cos_theta = max(dot(tile_view_dir, perturbed_n), 0.0);
        let fresnel = schlick_fresnel(cos_theta, 0.04) * 0.6;

        // Reflection: use smooth (unsnapped) normal & view so reflections aren't pixelated
        var smooth_n: vec3<f32>;
        if water.t > WATER_NORMAL_FLAT_DIST {
            smooth_n = vec3<f32>(0.0, 1.0, 0.0);
        } else if water.t > WATER_NORMAL_LOD2_DIST {
            smooth_n = water_normal_lod(surface_pos.xz, camera.time, 1);
        } else if water.t > WATER_NORMAL_LOD1_DIST {
            smooth_n = water_normal_lod(surface_pos.xz, camera.time, 2);
        } else {
            smooth_n = water_normal(surface_pos.xz, camera.time);
        }
        let smooth_view_dir = normalize(camera.camera_pos - surface_pos);
        let smooth_ray_dir = -smooth_view_dir;
        let reflect_dir = reflect(smooth_ray_dir, smooth_n);

        var refl_color: vec3<f32>;
        if water.t > WATER_NORMAL_FLAT_DIST {
            // Extreme distance — cheap constant
            refl_color = WATER_DEEP_COLOR;
        } else if render_settings.water_reflections > 0.5 && water.t < WATER_REFL_SKIP_DIST {
            // Ray-traced reflection: trace into the scene from the water surface
            let refl_origin = surface_pos + smooth_n * render_settings.ray_epsilon;
            let refl_hit = trace_reflection_ray(refl_origin, reflect_dir, render_settings.water_reflection_distance, surface_pos.y);
            if refl_hit.hit {
                // Apply the same lighting model as the lighting pass:
                // ambient + directional * NdotL * shadow
                let refl_ndotl = max(dot(refl_hit.normal, sun_dir), 0.0);
                var refl_shadow = 1.0;
                if render_settings.sun_contribution > 0.0 {
                    let refl_shadow_origin = refl_hit.world_pos + refl_hit.normal * render_settings.ray_epsilon;
                    refl_shadow = select(1.0, 0.0, trace_shadow_ray(refl_shadow_origin, sun_dir));
                }
                let refl_light = render_settings.ambient_light + render_settings.sun_contribution * refl_ndotl * refl_shadow;
                refl_color = refl_hit.color * refl_light;
            } else {
                if ENABLE_TRACE_STATS { trace_stats_water_sky_evals += 1u; }
                refl_color = sky_color(reflect_dir, sun_dir) * 0.4;
            }
        } else {
            if ENABLE_TRACE_STATS { trace_stats_water_sky_evals += 1u; }
            refl_color = sky_color(reflect_dir, sun_dir) * 0.4;
        }

        let half_vec = normalize(tile_view_dir + sun_dir);
        let spec_base = pow(max(dot(perturbed_n, half_vec), 0.0), 256.0) * render_settings.sun_contribution * 0.3;
        // Fade specular with distance so distant glare doesn't blow out
        let spec_fade = 1.0 / (1.0 + water.t * 0.01);
        let spec = spec_base * spec_fade;
        let specular = vec3<f32>(1.0, 0.95, 0.8) * spec;

        water_color = mix(refraction, refl_color, fresnel) + specular;
        water_n = perturbed_n;
    } else {
        if ENABLE_TRACE_STATS { trace_stats_water_side_face_pixels += 1u; }
        // --- Side-face: simple absorption view through the water wall ---
        water_color = refraction;
        water_n = water.entry_normal;
    }

    // Shadow ray from water surface (skip if disabled or distant water)
    var shadow_ray_count = 0u;
    var shadow_blocked_count = 0u;
    if render_settings.water_shadows > 0.5 && render_settings.sun_contribution > 0.0 && water.t < render_settings.water_shadow_distance {
        if ENABLE_TRACE_STATS { trace_stats_water_shadow_rays += 1u; }
        let shadow_origin = surface_pos + water.entry_normal * render_settings.ray_epsilon;
        let in_shadow = trace_shadow_ray(shadow_origin, sun_dir);
        shadow_ray_count = 1u;
        shadow_blocked_count = select(0u, 1u, in_shadow);
        shadow = select(1.0, 0.0, in_shadow);
    } else if ENABLE_TRACE_STATS && render_settings.sun_contribution > 0.0 {
        trace_stats_water_shadow_skipped += 1u;
    }

    // Water flag: normal.w = 0.5 tells the lighting pass this is pre-lit water
    let md = compute_motion_depth(surface_pos);
    write_gbuffer(pixel, water_color, shadow, water_n, 0.5, water.t, md);
    return ShadowCounts(shadow_ray_count, shadow_blocked_count);
}

fn shade_sky(pixel: vec2<i32>, ray_origin: vec3<f32>, ray_dir: vec3<f32>) -> ShadowCounts {
    // Place at a very far world-space point along the ray
    // so DLSS computes correct temporal reprojection for sky pixels
    let sky_pos = camera.camera_pos + ray_dir * 100000.0;
    let sky_md_raw = compute_motion_depth(sky_pos);
    // Force hardware depth to exactly 1.0 for sky (far plane)
    let sky_md = MotionDepth(sky_md_raw.motion, 1.0);

    if camera.camera_underwater > 0.5 {
        if ENABLE_TRACE_STATS { trace_stats_water_underwater_sky += 1u; }
        let underwater_sky = underwater_sky_view(ray_origin, ray_dir);
        write_gbuffer(pixel, underwater_sky, 1.0, vec3<f32>(0.0, 1.0, 0.0), 0.5, 0.0, sky_md);
    } else {
        write_gbuffer(pixel, vec3<f32>(0.0), 0.0, vec3<f32>(0.0), -1.0, 0.0, sky_md);
    }
    return ShadowCounts(0u, 0u);
}
