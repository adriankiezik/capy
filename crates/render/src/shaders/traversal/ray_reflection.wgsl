struct ReflectionHit {
    hit: bool,
    color: vec3<f32>,
    normal: vec3<f32>,
    world_pos: vec3<f32>,
};

fn trace_reflection_ray(
    ray_origin: vec3<f32>,
    ray_dir: vec3<f32>,
    max_dist: f32,
    water_y: f32,
) -> ReflectionHit {
    var refl: ReflectionHit;
    refl.hit = false;
    refl.color = vec3<f32>(0.0);
    refl.normal = vec3<f32>(0.0, 1.0, 0.0);
    refl.world_pos = vec3<f32>(0.0);

    if !chunk_dda_init(ray_origin, ray_dir) { return refl; }

    let do_grass = render_settings.vegetation_enabled > 0.5;
    let grass_blade_height =
        GRASS_BLADE_HEIGHT * render_settings.vegetation_length * render_settings.vegetation_scale;
    var best_grass: GrassHit;
    best_grass.hit = false;
    best_grass.t = 1e20;

    let max_steps = u32(max(round(render_settings.max_chunk_steps), 1.0));
    for (var i = 0u; i < max_steps; i++) {
        if do_grass && best_grass.hit && chunk_dda_t_enter() >= best_grass.t {
            break;
        }

        let info = lookup_chunk_info(dda.cc);

        if info.world_size != 0u {
            let chunk_min = chunk_dda_chunk_min();
            let local_origin = ray_origin - chunk_min;
            let t_enter = chunk_dda_t_enter();

            let chunk_hit = traverse_chunk(
                info.pool_offset, info.world_size, info.root_offset, info.depth,
                local_origin, dda.dir, t_enter, dda.entry_axis,
            );

            let chunk_t_exit = chunk_dda_t_exit();

            if chunk_hit.hit {
                if do_grass && info.foliage_y_min < info.foliage_y_max {
                    let voxel_t = chunk_hit.t;
                    let grass_max = select(voxel_t, min(voxel_t, best_grass.t), best_grass.hit);
                    let foliage_base_y = chunk_min.y + f32(info.foliage_y_min);
                    let foliage_top_y = chunk_min.y + f32(info.foliage_y_max) + grass_blade_height;
                    let grass = trace_grass_ray_bounded(
                        ray_origin, dda.dir, camera.time, grass_max,
                        foliage_base_y, foliage_top_y,
                        t_enter, chunk_t_exit,
                        info.foliage_bitmap_offset, chunk_min.x, chunk_min.z, dda.cs_xz,
                        chunk_min.y, info.foliage_y_bands,
                        info.foliage_tile_y_ranges_offset,
                    );
                    if grass.hit && grass.t < best_grass.t {
                        best_grass = grass;
                    }
                }

                if best_grass.hit && best_grass.t < chunk_hit.t {
                    if best_grass.pos.y >= water_y {
                        refl.hit = true;
                        refl.color = best_grass.color;
                        refl.normal = best_grass.normal;
                        refl.world_pos = best_grass.pos;
                    }
                    return refl;
                }

                let hit_world_pos = chunk_hit.hit_pos_local + chunk_min;
                if hit_world_pos.y >= water_y {
                    refl.hit = true;
                    refl.normal = chunk_hit.normal;
                    refl.world_pos = hit_world_pos;
                    if chunk_hit.is_lod_hit {
                        refl.color = chunk_hit.color_override;
                    } else {
                        refl.color = render_settings.material_colors[min(chunk_hit.material & 0x3FFFu, 1023u)].rgb;
                    }
                    return refl;
                }
                return refl;
            } else if do_grass && info.foliage_y_min < info.foliage_y_max {
                let grass_max = select(1e20, best_grass.t, best_grass.hit);
                let foliage_base_y = chunk_min.y + f32(info.foliage_y_min);
                let foliage_top_y = chunk_min.y + f32(info.foliage_y_max) + grass_blade_height;
                let grass = trace_grass_ray_bounded(
                    ray_origin, dda.dir, camera.time, grass_max,
                    foliage_base_y, foliage_top_y,
                    t_enter, chunk_t_exit,
                    info.foliage_bitmap_offset, chunk_min.x, chunk_min.z, dda.cs_xz,
                    chunk_min.y, info.foliage_y_bands,
                    info.foliage_tile_y_ranges_offset,
                );
                if grass.hit && grass.t < best_grass.t {
                    best_grass = grass;
                }
            }
        }

        if !chunk_dda_step() || dda.t_current >= max_dist { break; }
    }

    if best_grass.hit && best_grass.pos.y >= water_y {
        refl.hit = true;
        refl.color = best_grass.color;
        refl.normal = best_grass.normal;
        refl.world_pos = best_grass.pos;
    }

    return refl;
}
