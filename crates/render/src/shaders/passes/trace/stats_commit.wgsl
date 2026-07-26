// Workgroup-level reduction: 64 threads accumulate into shared memory,
// then a single thread commits 36 global atomics instead of 64×36.
const NUM_TRACE_STAT_FIELDS: u32 = 36u;
var<workgroup> wg_stats: array<atomic<u32>, 36>;

fn commit_trace_stats(
    lid: u32,
    in_bounds: bool,
    hit: HitResult,
    shadow_rays: u32,
    shadow_blocked: u32,
) {
    if !ENABLE_TRACE_STATS { return; }

    // Zero workgroup accumulators (first 36 threads each clear one slot)
    if lid < NUM_TRACE_STAT_FIELDS {
        atomicStore(&wg_stats[lid], 0u);
    }
    workgroupBarrier();

    // Each thread adds its private counters to workgroup-local atomics
    atomicAdd(&wg_stats[0], trace_stats_primary_chunk_steps);
    atomicAdd(&wg_stats[1], trace_stats_primary_node_steps);
    atomicAdd(&wg_stats[2], trace_stats_primary_descents);
    atomicAdd(&wg_stats[3], trace_stats_shadow_chunk_steps);
    atomicAdd(&wg_stats[4], trace_stats_shadow_node_steps);
    atomicAdd(&wg_stats[5], trace_stats_shadow_descents);
    if in_bounds {
        if hit.hit {
            atomicAdd(&wg_stats[6], 1u);
            atomicAdd(&wg_stats[11], 1u);
        } else {
            atomicAdd(&wg_stats[7], 1u);
        }
    }
    if shadow_rays > 0u {
        atomicAdd(&wg_stats[8], shadow_rays);
    }
    if shadow_blocked > 0u {
        atomicAdd(&wg_stats[9], shadow_blocked);
    }
    atomicAdd(&wg_stats[12], trace_stats_grass_trace_calls);
    atomicAdd(&wg_stats[13], trace_stats_grass_run_visits);
    atomicAdd(&wg_stats[14], trace_stats_grass_steps);
    atomicAdd(&wg_stats[15], trace_stats_grass_candidates);
    atomicAdd(&wg_stats[16], trace_stats_grass_tile_rejects);
    atomicAdd(&wg_stats[17], trace_stats_grass_heightmap_reads);
    atomicAdd(&wg_stats[18], trace_stats_grass_column_misses);
    atomicAdd(&wg_stats[19], trace_stats_grass_y_checks);
    atomicAdd(&wg_stats[20], trace_stats_grass_y_rejects);
    atomicAdd(&wg_stats[21], trace_stats_grass_trace_hits);
    atomicAdd(&wg_stats[22], trace_stats_grass_visible_pixels);
    atomicAdd(&wg_stats[23], trace_stats_grass_shadow_rays);
    atomicAdd(&wg_stats[24], trace_stats_water_pixels);
    atomicAdd(&wg_stats[25], trace_stats_water_top_face_pixels);
    atomicAdd(&wg_stats[26], trace_stats_water_side_face_pixels);
    atomicAdd(&wg_stats[27], trace_stats_water_shadow_rays);
    atomicAdd(&wg_stats[28], trace_stats_water_absorb_evals);
    atomicAdd(&wg_stats[29], trace_stats_water_underwater_sky);
    atomicAdd(&wg_stats[30], trace_stats_water_dda_chunks_behind);
    atomicAdd(&wg_stats[31], trace_stats_water_deep_no_hit);
    atomicAdd(&wg_stats[32], trace_stats_water_normal_evals);
    atomicAdd(&wg_stats[33], trace_stats_water_sky_evals);
    atomicAdd(&wg_stats[34], trace_stats_water_normal_lod);
    atomicAdd(&wg_stats[35], trace_stats_water_shadow_skipped);

    workgroupBarrier();

    // Single thread commits workgroup totals to global buffer (36 vs 64×36 atomics)
    if lid == 0u {
        atomicAdd(&trace_stats.primary_chunk_steps, atomicLoad(&wg_stats[0]));
        atomicAdd(&trace_stats.primary_node_steps, atomicLoad(&wg_stats[1]));
        atomicAdd(&trace_stats.primary_descents, atomicLoad(&wg_stats[2]));
        atomicAdd(&trace_stats.shadow_chunk_steps, atomicLoad(&wg_stats[3]));
        atomicAdd(&trace_stats.shadow_node_steps, atomicLoad(&wg_stats[4]));
        atomicAdd(&trace_stats.shadow_descents, atomicLoad(&wg_stats[5]));
        atomicAdd(&trace_stats.hit_pixels, atomicLoad(&wg_stats[6]));
        atomicAdd(&trace_stats.miss_pixels, atomicLoad(&wg_stats[7]));
        atomicAdd(&trace_stats.shadow_rays, atomicLoad(&wg_stats[8]));
        atomicAdd(&trace_stats.shadow_blocked, atomicLoad(&wg_stats[9]));
        atomicAdd(&trace_stats.lod_hits, atomicLoad(&wg_stats[10]));
        atomicAdd(&trace_stats.material_hits, atomicLoad(&wg_stats[11]));
        atomicAdd(&trace_stats.grass_trace_calls, atomicLoad(&wg_stats[12]));
        atomicAdd(&trace_stats.grass_run_visits, atomicLoad(&wg_stats[13]));
        atomicAdd(&trace_stats.grass_steps, atomicLoad(&wg_stats[14]));
        atomicAdd(&trace_stats.grass_candidates, atomicLoad(&wg_stats[15]));
        atomicAdd(&trace_stats.grass_tile_rejects, atomicLoad(&wg_stats[16]));
        atomicAdd(&trace_stats.grass_heightmap_reads, atomicLoad(&wg_stats[17]));
        atomicAdd(&trace_stats.grass_column_misses, atomicLoad(&wg_stats[18]));
        atomicAdd(&trace_stats.grass_y_checks, atomicLoad(&wg_stats[19]));
        atomicAdd(&trace_stats.grass_y_rejects, atomicLoad(&wg_stats[20]));
        atomicAdd(&trace_stats.grass_trace_hits, atomicLoad(&wg_stats[21]));
        atomicAdd(&trace_stats.grass_visible_pixels, atomicLoad(&wg_stats[22]));
        atomicAdd(&trace_stats.grass_shadow_rays, atomicLoad(&wg_stats[23]));
        atomicAdd(&trace_stats.water_pixels, atomicLoad(&wg_stats[24]));
        atomicAdd(&trace_stats.water_top_face_pixels, atomicLoad(&wg_stats[25]));
        atomicAdd(&trace_stats.water_side_face_pixels, atomicLoad(&wg_stats[26]));
        atomicAdd(&trace_stats.water_shadow_rays, atomicLoad(&wg_stats[27]));
        atomicAdd(&trace_stats.water_absorb_evals, atomicLoad(&wg_stats[28]));
        atomicAdd(&trace_stats.water_underwater_sky, atomicLoad(&wg_stats[29]));
        atomicAdd(&trace_stats.water_dda_chunks_behind, atomicLoad(&wg_stats[30]));
        atomicAdd(&trace_stats.water_deep_no_hit, atomicLoad(&wg_stats[31]));
        atomicAdd(&trace_stats.water_normal_evals, atomicLoad(&wg_stats[32]));
        atomicAdd(&trace_stats.water_sky_evals, atomicLoad(&wg_stats[33]));
        atomicAdd(&trace_stats.water_normal_lod, atomicLoad(&wg_stats[34]));
        atomicAdd(&trace_stats.water_shadow_skipped, atomicLoad(&wg_stats[35]));
    }
}
