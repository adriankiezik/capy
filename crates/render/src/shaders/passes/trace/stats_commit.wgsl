fn commit_trace_stats(
    hit: HitResult,
    shadow_rays: u32,
    shadow_blocked: u32,
) {
    if !ENABLE_TRACE_STATS { return; }
    atomicAdd(&trace_stats.primary_chunk_steps, trace_stats_primary_chunk_steps);
    atomicAdd(&trace_stats.primary_node_steps, trace_stats_primary_node_steps);
    atomicAdd(&trace_stats.primary_descents, trace_stats_primary_descents);
    atomicAdd(&trace_stats.shadow_chunk_steps, trace_stats_shadow_chunk_steps);
    atomicAdd(&trace_stats.shadow_node_steps, trace_stats_shadow_node_steps);
    atomicAdd(&trace_stats.shadow_descents, trace_stats_shadow_descents);
    if hit.hit {
        atomicAdd(&trace_stats.hit_pixels, 1u);
        if hit.is_lod_hit {
            atomicAdd(&trace_stats.lod_hits, 1u);
        } else {
            atomicAdd(&trace_stats.material_hits, 1u);
        }
    } else {
        atomicAdd(&trace_stats.miss_pixels, 1u);
    }
    if shadow_rays > 0u {
        atomicAdd(&trace_stats.shadow_rays, shadow_rays);
    }
    if shadow_blocked > 0u {
        atomicAdd(&trace_stats.shadow_blocked, shadow_blocked);
    }
    atomicAdd(&trace_stats.grass_trace_calls, trace_stats_grass_trace_calls);
    atomicAdd(&trace_stats.grass_run_visits, trace_stats_grass_run_visits);
    atomicAdd(&trace_stats.grass_steps, trace_stats_grass_steps);
    atomicAdd(&trace_stats.grass_candidates, trace_stats_grass_candidates);
    atomicAdd(&trace_stats.grass_tile_rejects, trace_stats_grass_tile_rejects);
    atomicAdd(&trace_stats.grass_heightmap_reads, trace_stats_grass_heightmap_reads);
    atomicAdd(&trace_stats.grass_column_misses, trace_stats_grass_column_misses);
    atomicAdd(&trace_stats.grass_y_checks, trace_stats_grass_y_checks);
    atomicAdd(&trace_stats.grass_y_rejects, trace_stats_grass_y_rejects);
    atomicAdd(&trace_stats.grass_trace_hits, trace_stats_grass_trace_hits);
    atomicAdd(&trace_stats.grass_visible_pixels, trace_stats_grass_visible_pixels);
    atomicAdd(&trace_stats.grass_shadow_rays, trace_stats_grass_shadow_rays);
    atomicAdd(&trace_stats.water_pixels, trace_stats_water_pixels);
    atomicAdd(&trace_stats.water_top_face_pixels, trace_stats_water_top_face_pixels);
    atomicAdd(&trace_stats.water_side_face_pixels, trace_stats_water_side_face_pixels);
    atomicAdd(&trace_stats.water_shadow_rays, trace_stats_water_shadow_rays);
    atomicAdd(&trace_stats.water_absorb_evals, trace_stats_water_absorb_evals);
    atomicAdd(&trace_stats.water_underwater_sky, trace_stats_water_underwater_sky);
    atomicAdd(&trace_stats.water_dda_chunks_behind, trace_stats_water_dda_chunks_behind);
    atomicAdd(&trace_stats.water_deep_no_hit, trace_stats_water_deep_no_hit);
    atomicAdd(&trace_stats.water_normal_evals, trace_stats_water_normal_evals);
    atomicAdd(&trace_stats.water_sky_evals, trace_stats_water_sky_evals);
    atomicAdd(&trace_stats.water_normal_lod, trace_stats_water_normal_lod);
    atomicAdd(&trace_stats.water_shadow_skipped, trace_stats_water_shadow_skipped);
}
