var<private> trace_stats_primary_chunk_steps: u32;
var<private> trace_stats_primary_node_steps: u32;
var<private> trace_stats_primary_descents: u32;
var<private> trace_stats_primary_occupied_chunks: u32;
var<private> trace_stats_primary_empty_chunks: u32;

fn reset_primary_only_trace_stats() {
    if !ENABLE_TRACE_STATS {
        return;
    }
    trace_stats_primary_chunk_steps = 0u;
    trace_stats_primary_node_steps = 0u;
    trace_stats_primary_descents = 0u;
    trace_stats_primary_occupied_chunks = 0u;
    trace_stats_primary_empty_chunks = 0u;
}

fn trace_ray_primary_only(ray_origin: vec3<f32>, ray_dir: vec3<f32>, t_start: f32) -> HitResult {
    var result: HitResult;
    result.hit = false;

    if ray_origin.y < 0.0 { return result; }
    if !chunk_dda_init(ray_origin, ray_dir, t_start) { return result; }

    let max_steps = u32(max(round(render_settings.max_chunk_steps), 1.0));
    for (var i = 0u; i < max_steps; i++) {
        if ENABLE_TRACE_STATS {
            trace_stats_primary_chunk_steps += 1u;
        }
        let info = lookup_chunk_info(dda.cc);

        if info.world_size != 0u {
            if ENABLE_TRACE_STATS {
                trace_stats_primary_occupied_chunks += 1u;
            }
            if !slot_solid_aabb_valid(info) {
                if !chunk_dda_step() { break; }
                continue;
            }

            let chunk_min = chunk_dda_chunk_min();
            let local_origin = ray_origin - chunk_min;
            let solid_t = intersect_aabb(
                local_origin,
                dda.dir,
                slot_solid_aabb_min(info),
                slot_solid_aabb_max(info),
            );
            let chunk_t_enter = chunk_dda_t_enter();
            let chunk_t_exit = chunk_dda_t_exit();
            if solid_t.x >= solid_t.y || solid_t.y <= chunk_t_enter || solid_t.x >= chunk_t_exit {
                if !chunk_dda_step() { break; }
                continue;
            }

            let trace_t_enter = max(chunk_t_enter, solid_t.x);
            var trace_entry_axis = dda.entry_axis;
            if solid_t.x > chunk_t_enter {
                trace_entry_axis = slot_solid_aabb_entry_axis(info, local_origin, dda.dir);
            }

            let pool_base = info.pool_offset;
            let root_flags = get_node_flags_pool(pool_base, info.root_offset);

            let chunk_hit = traverse_chunk_solid(
                pool_base, info.world_size, info.root_offset, info.depth,
                local_origin, dda.dir, trace_t_enter, trace_entry_axis,
            );

            if chunk_hit.hit {
                var world_hit = chunk_hit;
                world_hit.hit_pos_local = chunk_hit.hit_pos_local + chunk_min;
                return world_hit;
            }
        } else if ENABLE_TRACE_STATS {
            trace_stats_primary_empty_chunks += 1u;
        }

        if !chunk_dda_step() { break; }
    }

    return result;
}
