// Sized for max tree depth 6 (chunk dims up to 4096). Re-indexed via
// (root_se - scale_exp) >> 1  so entries pack from index 0.
var<private> stk: array<StackEntry, 6>;
// Best grass hit found during DDA traversal, read by trace.wgsl after trace_ray().
var<private> dda_grass_hit: GrassHit;
// When true, trace_ray() ignores grass entirely (used by pick shader for editor tools).
var<private> skip_grass: bool;

// Water voxel hit found during DDA traversal, read by trace.wgsl after trace_ray().
// The traversal skips water voxels (treats them as transparent) and records the
// closest water surface hit here. The solid hit behind water (if any) is returned
// normally as the HitResult from trace_ray().
var<private> dda_water_hit: WaterHit;

var<private> trace_stats_primary_chunk_steps: u32;
var<private> trace_stats_primary_node_steps: u32;
var<private> trace_stats_primary_descents: u32;
var<private> trace_stats_shadow_chunk_steps: u32;
var<private> trace_stats_shadow_node_steps: u32;
var<private> trace_stats_shadow_descents: u32;

fn reset_trace_private_stats() {
    trace_stats_primary_chunk_steps = 0u;
    trace_stats_primary_node_steps = 0u;
    trace_stats_primary_descents = 0u;
    trace_stats_shadow_chunk_steps = 0u;
    trace_stats_shadow_node_steps = 0u;
    trace_stats_shadow_descents = 0u;
    trace_stats_grass_trace_calls = 0u;
    trace_stats_grass_run_visits = 0u;
    trace_stats_grass_steps = 0u;
    trace_stats_grass_candidates = 0u;
    trace_stats_grass_tile_rejects = 0u;
    trace_stats_grass_heightmap_reads = 0u;
    trace_stats_grass_column_misses = 0u;
    trace_stats_grass_y_checks = 0u;
    trace_stats_grass_y_rejects = 0u;
    trace_stats_grass_trace_hits = 0u;
    trace_stats_grass_visible_pixels = 0u;
    trace_stats_grass_shadow_rays = 0u;
    trace_stats_water_pixels = 0u;
    trace_stats_water_top_face_pixels = 0u;
    trace_stats_water_side_face_pixels = 0u;
    trace_stats_water_shadow_rays = 0u;
    trace_stats_water_absorb_evals = 0u;
    trace_stats_water_underwater_sky = 0u;
    trace_stats_water_dda_chunks_behind = 0u;
    trace_stats_water_deep_no_hit = 0u;
    trace_stats_water_normal_evals = 0u;
    trace_stats_water_sky_evals = 0u;
    trace_stats_water_normal_lod = 0u;
    trace_stats_water_shadow_skipped = 0u;
}

fn record_water_surface_hit(
    ray_origin_world: vec3<f32>,
    ray_dir_world: vec3<f32>,
    water_pos_local: vec3<f32>,
    entry_axis: i32,
) {
    if render_settings.water_enabled <= 0.5 {
        return;
    }

    let wt = dot(water_pos_local - ray_origin_world, ray_dir_world);
    if !dda_water_hit.hit || wt < dda_water_hit.t {
        dda_water_hit.hit = true;
        dda_water_hit.t = wt;
        dda_water_hit.entry_normal = axis_normal(entry_axis, ray_dir_world);
    }
}
