const NUM_PRIMARY_TRACE_STAT_FIELDS: u32 = 9u;
var<workgroup> primary_wg_stats: array<atomic<u32>, 9>;

fn commit_primary_only_trace_stats(
    lid: u32,
    in_bounds: bool,
    hit: HitResult,
) {
    if !ENABLE_TRACE_STATS { return; }

    if lid < NUM_PRIMARY_TRACE_STAT_FIELDS {
        atomicStore(&primary_wg_stats[lid], 0u);
    }
    workgroupBarrier();

    atomicAdd(&primary_wg_stats[0], trace_stats_primary_chunk_steps);
    atomicAdd(&primary_wg_stats[1], trace_stats_primary_node_steps);
    atomicAdd(&primary_wg_stats[2], trace_stats_primary_descents);
    atomicAdd(&primary_wg_stats[3], trace_stats_primary_occupied_chunks);
    atomicAdd(&primary_wg_stats[4], trace_stats_primary_empty_chunks);
    if in_bounds {
        if hit.hit {
            atomicAdd(&primary_wg_stats[5], 1u);
            atomicAdd(&primary_wg_stats[8], 1u);
        } else {
            atomicAdd(&primary_wg_stats[6], 1u);
        }
    }

    workgroupBarrier();

    if lid == 0u {
        atomicAdd(&trace_stats.primary_chunk_steps, atomicLoad(&primary_wg_stats[0]));
        atomicAdd(&trace_stats.primary_node_steps, atomicLoad(&primary_wg_stats[1]));
        atomicAdd(&trace_stats.primary_descents, atomicLoad(&primary_wg_stats[2]));
        atomicAdd(&trace_stats.primary_occupied_chunks, atomicLoad(&primary_wg_stats[3]));
        atomicAdd(&trace_stats.primary_empty_chunks, atomicLoad(&primary_wg_stats[4]));
        atomicAdd(&trace_stats.hit_pixels, atomicLoad(&primary_wg_stats[5]));
        atomicAdd(&trace_stats.miss_pixels, atomicLoad(&primary_wg_stats[6]));
        atomicAdd(&trace_stats.lod_hits, atomicLoad(&primary_wg_stats[7]));
        atomicAdd(&trace_stats.material_hits, atomicLoad(&primary_wg_stats[8]));
    }
}
