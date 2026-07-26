fn trace_ray(ray_origin: vec3<f32>, ray_dir: vec3<f32>, t_start: f32) -> HitResult {
    return trace_ray_bounded(ray_origin, ray_dir, t_start, 1e20);
}

fn trace_ray_bounded(
    ray_origin: vec3<f32>,
    ray_dir: vec3<f32>,
    t_start: f32,
    max_t: f32,
) -> HitResult {
    if FEATURE_GRASS {
        dda_grass_hit.hit = false;
        dda_grass_hit.t = 1e20;
    }
    if FEATURE_WATER {
        dda_water_hit.hit = false;
    }

    var result: HitResult;
    result.hit = false;

    if ray_origin.y < 0.0 { return result; }
    if !chunk_dda_init(ray_origin, ray_dir, t_start) { return result; }

    let grass_blade_height =
        GRASS_BLADE_HEIGHT * render_settings.vegetation_length * render_settings.vegetation_scale;
    let max_steps = u32(max(round(render_settings.max_chunk_steps), 1.0));
    for (var i = 0u; i < max_steps; i++) {
        var do_grass = FEATURE_GRASS && !skip_grass;

        if chunk_dda_t_enter() > max_t {
            return result;
        }

        if do_grass && dda_grass_hit.hit && chunk_dda_t_enter() >= dda_grass_hit.t {
            return result;
        }

        if FEATURE_WATER && dda_water_hit.hit && camera.camera_underwater <= 0.5
            && (chunk_dda_t_enter() - dda_water_hit.t) > WATER_DEEP_ABSORB_DIST {
            return result;
        }

        if ENABLE_TRACE_STATS {
            trace_stats_primary_chunk_steps += 1u;
            if dda_water_hit.hit {
                trace_stats_water_dda_chunks_behind += 1u;
            }
        }
        let info = lookup_chunk_info(dda.cc);
        let chunk_center_xz = vec2<f32>(
            (f32(dda.cc.x) + 0.5) * dda.cs_xz,
            (f32(dda.cc.z) + 0.5) * dda.cs_xz,
        );
        let chunk_delta = camera.camera_pos.xz - chunk_center_xz;
        let use_near_mesh = use_near_mesh_handoff
            && (info.flags & 1u) != 0u
            && render_settings.hybrid_near_radius > 0.0
            && dot(chunk_delta, chunk_delta)
                <= render_settings.hybrid_near_radius * render_settings.hybrid_near_radius;

        if do_grass && FEATURE_WATER && dda_water_hit.hit && camera.camera_underwater <= 0.5 {
            let water_surface_y = ray_origin.y + dda.dir.y * dda_water_hit.t;
            let chunk_min_y = f32(dda.cc.y) * f32(streaming.chunk_size_y);
            let foliage_top_world_y = chunk_min_y + f32(info.foliage_y_max);
            if (water_surface_y - foliage_top_world_y) > WATER_GRASS_SKIP_DEPTH {
                do_grass = false;
            }
        }

        // The near-field mesh replaces only solid-voxel traversal. Grass is
        // procedural and is not part of that mesh, so keep tracing the chunk's
        // foliage metadata and clip it to the raster surface passed as max_t.
        if info.world_size != 0u && use_near_mesh
            && do_grass && info.foliage_y_min < info.foliage_y_max
        {
            let chunk_min = chunk_dda_chunk_min();
            let grass_max = min(
                max_t,
                select(max_t, dda_grass_hit.t, dda_grass_hit.hit),
            );
            let foliage_base_y = chunk_min.y + f32(info.foliage_y_min);
            let foliage_top_y = chunk_min.y + f32(info.foliage_y_max) + grass_blade_height;
            let grass = trace_grass_ray_bounded(
                ray_origin, dda.dir, camera.time, grass_max,
                foliage_base_y, foliage_top_y,
                chunk_dda_t_enter(), chunk_dda_t_exit(),
                info.foliage_bitmap_offset, chunk_min.x, chunk_min.z, dda.cs_xz,
                chunk_min.y, info.foliage_y_bands,
                info.foliage_tile_y_ranges_offset,
            );
            if grass.hit && grass.t < dda_grass_hit.t {
                dda_grass_hit = grass;
            }
            if dda_grass_hit.hit {
                return result;
            }
        }

        if info.world_size != 0u && !use_near_mesh {
            let pool_base = info.pool_offset;
            let root_flags = get_node_flags_pool(pool_base, info.root_offset);

            let chunk_min = chunk_dda_chunk_min();
            let local_origin = ray_origin - chunk_min;
            let t_enter = chunk_dda_t_enter();

            let chunk_hit = traverse_chunk(
                pool_base, info.world_size, info.root_offset, info.depth,
                local_origin, dda.dir, t_enter, dda.entry_axis,
            );

            let chunk_t_exit = chunk_dda_t_exit();

            if chunk_hit.hit {
                if do_grass && info.foliage_y_min < info.foliage_y_max {
                    let voxel_t = chunk_hit.t;
                    let grass_max = select(voxel_t, min(voxel_t, dda_grass_hit.t), dda_grass_hit.hit);
                    let foliage_base_y = chunk_min.y + f32(info.foliage_y_min);
                    let foliage_top_y = chunk_min.y + f32(info.foliage_y_max) + grass_blade_height;
                    let grass = trace_grass_ray_bounded(
                        ray_origin, dda.dir, camera.time, grass_max,
                        foliage_base_y, foliage_top_y,
                        t_enter, chunk_t_exit,
                        info.foliage_bitmap_offset, chunk_min.x, chunk_min.z, f32(streaming.chunk_size_xz),
                        chunk_min.y, info.foliage_y_bands,
                        info.foliage_tile_y_ranges_offset,
                    );
                    if grass.hit && grass.t < dda_grass_hit.t {
                        dda_grass_hit = grass;
                    }
                }

                if !do_grass || !dda_grass_hit.hit || chunk_hit.t <= dda_grass_hit.t {
                    var world_hit = chunk_hit;
                    world_hit.hit_pos_local = chunk_hit.hit_pos_local + chunk_min;
                    return world_hit;
                }
                return result;
            } else if do_grass && info.foliage_y_min < info.foliage_y_max {
                let grass_max = select(1e20, dda_grass_hit.t, dda_grass_hit.hit);
                let foliage_base_y = chunk_min.y + f32(info.foliage_y_min);
                let foliage_top_y = chunk_min.y + f32(info.foliage_y_max) + grass_blade_height;
                let grass = trace_grass_ray_bounded(
                    ray_origin, dda.dir, camera.time, grass_max,
                    foliage_base_y, foliage_top_y,
                    t_enter, chunk_t_exit,
                    info.foliage_bitmap_offset, chunk_min.x, chunk_min.z, f32(streaming.chunk_size_xz),
                    chunk_min.y, info.foliage_y_bands,
                    info.foliage_tile_y_ranges_offset,
                );
                if grass.hit && grass.t < dda_grass_hit.t {
                    dda_grass_hit = grass;
                }
                if dda_grass_hit.hit {
                    return result;
                }
            }
        }

        if !chunk_dda_step() { break; }
    }

    return result;
}
