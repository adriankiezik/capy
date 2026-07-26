@compute @workgroup_size(8, 8)
fn main(
    @builtin(global_invocation_id) gid: vec3<u32>,
    @builtin(local_invocation_index) lid: u32,
) {
    let dims = textureDimensions(gbuf_color_out);
    let actual_x = gid.x;
    let actual_y = gid.y;
    let in_bounds = actual_x < dims.x && actual_y < dims.y;

    reset_trace_private_stats();
    skip_grass = render_settings.vegetation_enabled < 0.5;
    use_near_mesh_handoff = true;

    var hit: HitResult;
    var counts: ShadowCounts;

    if in_bounds {
        let uv_x = (f32(actual_x) + 0.5 + camera.jitter.x) / camera.resolution.x;
        let uv_y = 1.0 - (f32(actual_y) + 0.5 + camera.jitter.y) / camera.resolution.y;

        let ray_dir = normalize(
            camera.ray_corner
            + camera.ray_right * (uv_x * 2.0)
            + camera.ray_up * (uv_y * 2.0)
        );
        let ray_origin = camera.camera_pos;

        let pixel_idx = actual_y * u32(camera.resolution.x) + actual_x;
        let pixel = vec2<i32>(i32(actual_x), i32(actual_y));
        let mesh_depth = textureLoad(near_mesh_depth, pixel, 0).r;
        let trace_limit = select(1e20, mesh_depth, mesh_depth > 0.0);
        hit = trace_ray_bounded(ray_origin, ray_dir, trace_limit);
        let mesh_water_depth = textureLoad(near_mesh_water_depth, pixel, 0).r;
        if render_settings.water_enabled > 0.5
            && mesh_water_depth > 0.0
            && (!hit.hit || mesh_water_depth < hit.t)
            && (!dda_water_hit.hit || mesh_water_depth < dda_water_hit.t)
        {
            dda_water_hit.hit = true;
            dda_water_hit.t = mesh_water_depth;
            dda_water_hit.entry_normal = normalize(
                textureLoad(near_mesh_water_normal, pixel, 0).xyz,
            );
        }
        let sel = resolve_visible_hit(ray_origin, ray_dir, hit);
        let use_mesh = mesh_depth > 0.0 && mesh_depth <= visible_hit_depth(sel, hit);

        if use_mesh {
            let color = textureLoad(near_mesh_color, pixel, 0);
            let normal = textureLoad(near_mesh_normal, pixel, 0);
            let world_pos = ray_origin + ray_dir * mesh_depth;
            let shading_normal = normalize(normal.xyz);
            var shadow = 1.0;
            var shadow_ray_count = 0u;
            var shadow_blocked_count = 0u;
            let sun_dir = normalize(render_settings.sun_direction.xyz);
            if render_settings.sun_contribution > 0.0
                && dot(shading_normal, sun_dir) > 0.0
            {
                // A raster hit is reconstructed from an R32F ray depth. Its
                // world-space error grows with distance, so the tiny fixed bias
                // used by exact voxel hits eventually starts the shadow ray
                // inside its source voxel. Scale the bias with one pixel's
                // world-space footprint, while keeping it far below one voxel.
                let pixel_footprint = mesh_depth * camera.pixel_size;
                let shadow_bias = clamp(
                    max(render_settings.ray_epsilon, pixel_footprint * 0.002),
                    0.0001,
                    0.02,
                );
                let shadow_origin = world_pos + shading_normal * shadow_bias;
                let in_shadow = trace_shadow_ray(shadow_origin, sun_dir);
                shadow_ray_count = 1u;
                shadow_blocked_count = select(0u, 1u, in_shadow);
                shadow = select(1.0, 0.0, in_shadow);
            }
            let selected_color = apply_selection_tint(color.rgb, world_pos);
            let debug_tint_amount = 0.65
                * clamp(render_settings.hybrid_debug_tint, 0.0, 1.0);
            let tinted = mix(
                selected_color,
                vec3<f32>(1.0, 0.0, 0.75),
                debug_tint_amount,
            );
            let md = compute_motion_depth(world_pos);
            write_gbuffer(
                pixel,
                tinted,
                shadow,
                shading_normal,
                normal.w,
                mesh_depth,
                md,
            );
            hit.hit = true;
            hit.is_lod_hit = false;
            hit.lod_scale_exp = 0u;
            if pixel_idx < arrayLength(&lod_debug_buf) {
                lod_debug_buf[pixel_idx] = 0u;
            }
            counts = ShadowCounts(shadow_ray_count, shadow_blocked_count);
        } else {
            if pixel_idx < arrayLength(&lod_debug_buf) {
                lod_debug_buf[pixel_idx] = hit.lod_scale_exp;
            }

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
        }
    }

    // All threads must reach this call (workgroupBarrier inside)
    commit_trace_stats(lid, in_bounds, hit, counts.rays, counts.blocked);
}
