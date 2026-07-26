fn trace_shadow_ray(ray_origin: vec3<f32>, ray_dir: vec3<f32>) -> bool {
    if !chunk_dda_init(ray_origin, ray_dir, 0.0) { return false; }

    let max_steps = u32(max(round(render_settings.max_chunk_steps), 1.0));
    for (var i = 0u; i < max_steps; i++) {
        if ENABLE_TRACE_STATS { trace_stats_shadow_chunk_steps += 1u; }
        let info = lookup_chunk_info(dda.cc);

        if info.world_size != 0u {
            let chunk_min = chunk_dda_chunk_min();
            if traverse_chunk_shadow(
                info.pool_offset, info.world_size, info.root_offset, info.depth,
                ray_origin - chunk_min, dda.dir, chunk_dda_t_enter(),
            ) {
                return true;
            }
        }

        if !chunk_dda_step() { break; }
    }
    return false;
}
