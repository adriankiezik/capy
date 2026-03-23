// ---------- Pixel Grass ----------
// 2D billboard grass blades on top of grass-material voxel surfaces.
// Each blade is a stack of independent pixel squares that face the camera.
// Wind moves each square independently — squares never deform, only slide sideways.
// Grass is traced per-chunk during DDA traversal, scoped to the chunk's
// foliage Y range, so blades correctly occlude (and are occluded by) solid voxels.

const GRASS_MATERIAL_ID: u32 = 1u;
const GRASS_BLADE_HEIGHT: f32 = 5.0;        // height in voxel units
const GRASS_PIXEL_SIZE: f32 = 0.5;          // size of each pixel square on the blade

// Foliage bitmap sentinel values (must match Rust FOLIAGE_BITMAP_ALL / FOLIAGE_BITMAP_NONE).
const FOLIAGE_BMP_ALL: u32 = 0xFFFFFFFEu;  // all columns have foliage, no bitmap needed
const FOLIAGE_BMP_NONE: u32 = 0xFFFFFFFFu; // no foliage at all

// Sentinel: no per-tile Y-range data (must match Rust FOLIAGE_TILE_NONE).
const FOLIAGE_TILE_NONE: u32 = 0xFFFFFFFFu;
// Must match capy_world::bake::TILE_SIZE.
const FOLIAGE_TILE_SIZE_U32: u32 = 8u;

// Wind parameters
const GRASS_WIND_STRENGTH: f32 = 1.5;       // max sway in pixel-grid units at blade tip
const GRASS_WIND_SPEED: f32 = 2.0;          // base wind oscillation speed
// Precomputed: normalize(vec2(1.0, 0.6))
const GRASS_WIND_DIR_NORM: vec2<f32> = vec2<f32>(0.8574929257125441, 0.5144957554275265);
const GRASS_WIND_BASE_LEAN: f32 = 0.25;     // constant lean in wind direction

// Blade color ramp derived from material palette at runtime.
// Darkening/lightening factors applied to the foliage material color.
const GRASS_COLOR_DARKEN: f32  = 0.75;  // base is 75% of material color
const GRASS_COLOR_LIGHTEN: f32 = 1.0;   // tip matches material color

// Toggle to disable per-invocation trace-stat counters for shipping builds.
const ENABLE_TRACE_STATS: bool = false;

fn grass_hash(x: i32, z: i32) -> u32 {
    var h = u32(x) * 374761393u + u32(z) * 668265263u;
    h = (h ^ (h >> 13u)) * 1274126177u;
    h = h ^ (h >> 16u);
    return h;
}

fn grass_search_radius(t_sample: f32, near_radius: i32, far_radius: i32, far_start: f32) -> i32 {
    if t_sample >= far_start {
        return far_radius;
    }
    return near_radius;
}

fn grass_step_scale(segment_mid_t: f32, far_step_scale: f32, far_start: f32) -> f32 {
    let ramp = max(far_start, 1.0);
    let lod = clamp((segment_mid_t - far_start) / ramp, 0.0, 1.0);
    return mix(1.0, far_step_scale, lod);
}

struct GrassHit {
    hit: bool,
    t: f32,              // ray t-value for depth comparison with voxels
    color: vec3<f32>,
    pos: vec3<f32>,
    normal: vec3<f32>,
};

var<private> trace_stats_grass_trace_calls: u32;
var<private> trace_stats_grass_run_visits: u32;
var<private> trace_stats_grass_steps: u32;
var<private> trace_stats_grass_candidates: u32;
var<private> trace_stats_grass_tile_rejects: u32;
var<private> trace_stats_grass_heightmap_reads: u32;
var<private> trace_stats_grass_column_misses: u32;
var<private> trace_stats_grass_y_checks: u32;
var<private> trace_stats_grass_y_rejects: u32;
var<private> trace_stats_grass_trace_hits: u32;
var<private> trace_stats_grass_visible_pixels: u32;
var<private> trace_stats_grass_shadow_rays: u32;

// Trace grass blades within a bounded Y slab [foliage_base_y, slab_top_y],
// clamped to the chunk's ray segment [chunk_t_enter, chunk_t_exit].
// Called from the DDA loop per chunk that has foliage.
//
// Two paths:
// - Uniform: bmp_offset == FOLIAGE_BMP_ALL — simple XZ march, no memory reads.
// - Per-tile: tile DDA with per-tile Y-range rejection, heightmap reads.
fn trace_grass_ray_bounded(
    ray_origin: vec3<f32>,
    ray_dir: vec3<f32>,
    time: f32,
    max_t: f32,           // reject any grass hit beyond this t
    foliage_base_y: f32,  // chunk_min.y + foliage_y_min
    slab_top_y: f32,      // chunk_min.y + foliage_y_max + GRASS_BLADE_HEIGHT
    chunk_t_enter: f32,   // ray t at chunk entry
    chunk_t_exit: f32,    // ray t at chunk exit
    bmp_offset: u32,      // foliage heightmap pool offset (FOLIAGE_BMP_ALL or FOLIAGE_BMP_NONE for sentinels)
    chunk_min_x: f32,     // chunk world-space origin X
    chunk_min_z: f32,     // chunk world-space origin Z
    chunk_size: f32,      // chunk_size_xz as float
    chunk_min_y: f32,     // chunk world-space origin Y
    foliage_y_bands: u32, // 32-bit Y-occupancy mask (chunk-level)
    tile_y_ranges_offset: u32, // pool offset to per-tile Y-range data, or FOLIAGE_TILE_NONE
) -> GrassHit {
    var result: GrassHit;
    result.hit = false;
    result.t = 1e20;
    if ENABLE_TRACE_STATS { trace_stats_grass_trace_calls += 1u; }

    let vegetation_density = render_settings.vegetation_density;
    let vegetation_max_distance = render_settings.vegetation_max_distance;
    if foliage_y_bands == 0u || vegetation_density <= 0.0 || vegetation_max_distance <= 0.0 {
        return result;
    }

    let density_threshold_u = u32(vegetation_density * 65535.0);
    let far_step_scale = render_settings.vegetation_far_step_scale;
    let far_reduce_start = render_settings.vegetation_far_reduce_start;
    let near_search_radius = i32(render_settings.vegetation_near_search_radius);
    let far_search_radius = i32(render_settings.vegetation_far_search_radius);

    // Default surface Y for FOLIAGE_BMP_ALL (all columns at the same height).
    let default_surface_y = foliage_base_y + 1.0;

    // Find where the ray intersects the grass Y slab
    let inv_dy = 1.0 / ray_dir.y;
    let t_bottom = (default_surface_y - ray_origin.y) * inv_dy;
    let t_top = (slab_top_y - ray_origin.y) * inv_dy;
    let t_enter_slab = max(min(t_bottom, t_top), 0.0);
    let t_exit_slab = max(t_bottom, t_top);

    if t_exit_slab < 0.0 || t_enter_slab > t_exit_slab {
        return result;
    }

    // Camera-facing blade orientation (2D — blade_normal.y and blade_tangent.y are always 0)
    let ray_d_xz = vec2<f32>(ray_dir.x, ray_dir.z);
    let ray_xz_len = length(ray_d_xz);
    if ray_xz_len < 0.001 {
        return result; // looking straight up/down
    }
    let inv_ray_xz_len = 1.0 / ray_xz_len;
    let blade_n_xz = -ray_d_xz * inv_ray_xz_len;
    let blade_t_xz = vec2<f32>(-blade_n_xz.y, blade_n_xz.x);
    let ray_o_xz = vec2<f32>(ray_origin.x, ray_origin.z);

    // blade_normal.y == 0 → denom = -ray_xz_len, inv_denom = -1/ray_xz_len
    let inv_denom = -inv_ray_xz_len;

    // Precompute wind time terms (constant for all blades)
    let wind_dir = GRASS_WIND_DIR_NORM;
    let time_wind = time * GRASS_WIND_SPEED;
    let wind_bend_scale = GRASS_WIND_STRENGTH * GRASS_PIXEL_SIZE;

    // One XZ traversal for the chunk segment, clipped to the coarse foliage slab.
    let trace_t_start = max(max(t_enter_slab, chunk_t_enter), 0.0);
    let trace_t_end = min(min(t_exit_slab, chunk_t_exit), min(max_t, vegetation_max_distance));
    if trace_t_start >= trace_t_end { return result; }

    let chunk_size_u = u32(chunk_size);

    var best_t = max_t;
    var best_height_frac = 0.0;
    var best_color_var = 0.0;

    // ── Fast path: uniform grass (all columns at default_surface_y) ──
    if bmp_offset == FOLIAGE_BMP_ALL {
        let xz_span = ray_xz_len * (trace_t_end - trace_t_start);
        let step_scale = grass_step_scale(
            (trace_t_start + trace_t_end) * 0.5,
            far_step_scale,
            far_reduce_start,
        );
        let n_steps = max(i32(ceil(xz_span / step_scale)), 1);
        let dt = (trace_t_end - trace_t_start) / f32(n_steps);

        for (var step = 0; step < n_steps; step++) {
            if ENABLE_TRACE_STATS { trace_stats_grass_steps += 1u; }
            let t_sample = trace_t_start + (f32(step) + 0.5) * dt;
            if t_sample - 3.0 >= best_t { break; }

            let sample_pos = ray_origin + ray_dir * t_sample;
            let center_gx = i32(floor(sample_pos.x));
            let center_gz = i32(floor(sample_pos.z));

            let search_radius = grass_search_radius(
                t_sample,
                near_search_radius,
                far_search_radius,
                far_reduce_start,
            );

            for (var dz = -search_radius; dz <= search_radius; dz++) {
                for (var dx = -search_radius; dx <= search_radius; dx++) {
                    if ENABLE_TRACE_STATS { trace_stats_grass_candidates += 1u; }
                    let gx = center_gx + dx;
                    let gz = center_gz + dz;

                    let h = grass_hash(gx, gz);
                    if (h & 0xFFFFu) > density_threshold_u {
                        continue;
                    }

                    let jitter_x = f32((h >> 8u) & 0xFFu) / 255.0;
                    let jitter_z = f32((h >> 16u) & 0xFFu) / 255.0;

                    let blade_world_x = f32(gx) + jitter_x;
                    let blade_world_z = f32(gz) + jitter_z;
                    let rel_x = blade_world_x - chunk_min_x;
                    let rel_z = blade_world_z - chunk_min_z;
                    if rel_x < 0.0 || rel_x >= chunk_size || rel_z < 0.0 || rel_z >= chunk_size {
                        continue;
                    }

                    let blade_xz = vec2<f32>(blade_world_x, blade_world_z);
                    let t_plane = dot(blade_xz - ray_o_xz, blade_n_xz) * inv_denom;
                    if t_plane < trace_t_start || t_plane >= best_t || t_plane >= trace_t_end {
                        continue;
                    }

                    let height_var = 0.7 + 0.3 * f32((h >> 24u) & 0xFFu) / 255.0;
                    let blade_h = GRASS_BLADE_HEIGHT * height_var;

                    let ray_y_at_plane = ray_origin.y + ray_dir.y * t_plane;
                    if ENABLE_TRACE_STATS { trace_stats_grass_y_checks += 1u; }
                    let local_y = ray_y_at_plane - default_surface_y;
                    if local_y < 0.0 || local_y >= blade_h {
                        if ENABLE_TRACE_STATS { trace_stats_grass_y_rejects += 1u; }
                        continue;
                    }

                    let n_pixels = i32(ceil(blade_h / GRASS_PIXEL_SIZE));
                    let pixel_y = i32(floor(local_y / GRASS_PIXEL_SIZE));

                    let blade_width = GRASS_PIXEL_SIZE;

                    let phase = f32(h & 0xFFu) / 255.0 * 6.2831;
                    let height_frac = f32(pixel_y) / f32(max(n_pixels - 1, 1));

                    var sq_center_xz = blade_xz;
                    if t_plane < render_settings.vegetation_animation_distance {
                        // Per-blade frequency variation
                        let freq_var = 0.8 + 0.4 * f32((h >> 12u) & 0xFFu) / 255.0;
                        let blade_time = time_wind * freq_var;

                        // Spatial wind: coarse gust waves + fine turbulence
                        let coarse_travel = dot(blade_xz, wind_dir) * 0.05;
                        let fine_travel = dot(blade_xz, wind_dir) * 0.4;

                        // Gust envelope + local flutter (asymmetric: never sways backward)
                        let gust_envelope = max(sin(blade_time * 0.6 - coarse_travel), 0.0);
                        let flutter = sin(blade_time * 2.3 - fine_travel + phase);
                        let sway = GRASS_WIND_BASE_LEAN + gust_envelope * (0.5 + 0.5 * flutter);

                        // Cubic bend: stiffer base, flexible tip
                        let bend = height_frac * height_frac * height_frac;
                        let wind_factor = sway * bend * wind_bend_scale;
                        sq_center_xz = blade_xz + blade_t_xz * wind_factor;
                    }
                    let p_xz = ray_o_xz + ray_d_xz * t_plane;
                    let local_tang = dot(p_xz - sq_center_xz, blade_t_xz);
                    if abs(local_tang) > blade_width * 0.5 {
                        continue;
                    }

                    best_t = t_plane;
                    best_height_frac = height_frac;
                    best_color_var = 0.92 + 0.08 * f32((h >> 4u) & 0xFFu) / 255.0;
                }
            }
        }

        if best_t < max_t {
            if ENABLE_TRACE_STATS { trace_stats_grass_trace_hits += 1u; }
            result.hit = true;
            result.t = best_t;
            result.pos = ray_origin + ray_dir * best_t;
            result.normal = vec3<f32>(0.0, 1.0, 0.0);

            let mat_color = render_settings.material_colors[GRASS_MATERIAL_ID].rgb;
            let ramp = mix(GRASS_COLOR_DARKEN, GRASS_COLOR_LIGHTEN, best_height_frac);
            result.color = min(mat_color * ramp, vec3<f32>(1.0)) * best_color_var;
        }
        return result;
    }

    // ── Tile DDA path: per-tile Y-range rejection + heightmap reads ──
    let tiles_per_axis = (chunk_size_u + FOLIAGE_TILE_SIZE_U32 - 1u) / FOLIAGE_TILE_SIZE_U32;
    let tiles_per_axis_i = i32(tiles_per_axis);
    let tile_size = f32(FOLIAGE_TILE_SIZE_U32);

    let tile_index_t = min(trace_t_end, trace_t_start + 0.0001);
    let tile_index_pos = ray_origin + ray_dir * tile_index_t;
    let local_tile_pos = vec2<f32>(
        clamp(tile_index_pos.x - chunk_min_x, 0.0, chunk_size - 0.0001),
        clamp(tile_index_pos.z - chunk_min_z, 0.0, chunk_size - 0.0001),
    );

    var tile_x = i32(clamp(
        floor(local_tile_pos.x / tile_size),
        0.0,
        f32(tiles_per_axis_i - 1),
    ));
    var tile_z = i32(clamp(
        floor(local_tile_pos.y / tile_size),
        0.0,
        f32(tiles_per_axis_i - 1),
    ));

    let inf_t = 1e20;
    var step_x = 0;
    var step_z = 0;
    var t_next_x = inf_t;
    var t_next_z = inf_t;
    var t_delta_x = inf_t;
    var t_delta_z = inf_t;

    if ray_dir.x > 0.0 {
        step_x = 1;
        let boundary_x = f32(tile_x + 1) * tile_size;
        t_next_x = tile_index_t + (boundary_x - local_tile_pos.x) / ray_dir.x;
        t_delta_x = tile_size / ray_dir.x;
    } else if ray_dir.x < 0.0 {
        step_x = -1;
        let boundary_x = f32(tile_x) * tile_size;
        t_next_x = tile_index_t + (boundary_x - local_tile_pos.x) / ray_dir.x;
        t_delta_x = tile_size / -ray_dir.x;
    }

    if ray_dir.z > 0.0 {
        step_z = 1;
        let boundary_z = f32(tile_z + 1) * tile_size;
        t_next_z = tile_index_t + (boundary_z - local_tile_pos.y) / ray_dir.z;
        t_delta_z = tile_size / ray_dir.z;
    } else if ray_dir.z < 0.0 {
        step_z = -1;
        let boundary_z = f32(tile_z) * tile_size;
        t_next_z = tile_index_t + (boundary_z - local_tile_pos.y) / ray_dir.z;
        t_delta_z = tile_size / -ray_dir.z;
    }

    var t_current = trace_t_start;
    loop {
        if t_current >= trace_t_end || t_current >= best_t {
            break;
        }
        if tile_x < 0 || tile_x >= tiles_per_axis_i || tile_z < 0 || tile_z >= tiles_per_axis_i {
            break;
        }

        let tile_t_enter = t_current;
        let tile_t_exit = min(trace_t_end, min(t_next_x, t_next_z));
        let hit_x = abs(t_next_x - tile_t_exit) <= 0.0001;
        let hit_z = abs(t_next_z - tile_t_exit) <= 0.0001;

        if tile_t_exit > tile_t_enter + 0.0001 {
            // Per-tile Y-range rejection
            let tile_idx = u32(tile_x) + u32(tile_z) * tiles_per_axis;
            var tile_skip = false;

            // Read per-tile min/max Y range for tight rejection
            var tile_min_y = foliage_base_y;
            var tile_max_y_top = slab_top_y;
            if tile_y_ranges_offset != FOLIAGE_TILE_NONE {
                let tile_data = chunk_pool[tile_y_ranges_offset + tile_idx];
                if tile_data == FOLIAGE_TILE_NONE {
                    tile_skip = true;
                } else {
                    tile_min_y = foliage_base_y + f32(tile_data & 0xFFFFu) + 1.0;
                    tile_max_y_top = foliage_base_y + f32(tile_data >> 16u) + 1.0 + GRASS_BLADE_HEIGHT;
                    let ray_y_enter = ray_origin.y + ray_dir.y * tile_t_enter;
                    let ray_y_exit  = ray_origin.y + ray_dir.y * tile_t_exit;
                    let ray_y_lo = min(ray_y_enter, ray_y_exit);
                    let ray_y_hi = max(ray_y_enter, ray_y_exit);
                    if ray_y_hi < tile_min_y || ray_y_lo > tile_max_y_top {
                        tile_skip = true;
                    }
                }
            }

            if !tile_skip {
                if ENABLE_TRACE_STATS { trace_stats_grass_run_visits += 1u; }

                let xz_span = ray_xz_len * (tile_t_exit - tile_t_enter);
                let step_scale = grass_step_scale(
                    (tile_t_enter + tile_t_exit) * 0.5,
                    far_step_scale,
                    far_reduce_start,
                );
                let n_steps = max(i32(ceil(xz_span / step_scale)), 1);
                let dt = (tile_t_exit - tile_t_enter) / f32(n_steps);

                for (var step = 0; step < n_steps; step++) {
                    if ENABLE_TRACE_STATS { trace_stats_grass_steps += 1u; }
                    let t_sample = tile_t_enter + (f32(step) + 0.5) * dt;
                    if t_sample - 3.0 >= best_t { break; }

                    let sample_pos = ray_origin + ray_dir * t_sample;
                    let center_gx = i32(floor(sample_pos.x));
                    let center_gz = i32(floor(sample_pos.z));

                    let search_radius = grass_search_radius(
                        t_sample,
                        near_search_radius,
                        far_search_radius,
                        far_reduce_start,
                    );

                    for (var dz = -search_radius; dz <= search_radius; dz++) {
                        for (var dx = -search_radius; dx <= search_radius; dx++) {
                            if ENABLE_TRACE_STATS { trace_stats_grass_candidates += 1u; }
                            let gx = center_gx + dx;
                            let gz = center_gz + dz;

                            let h = grass_hash(gx, gz);
                            if (h & 0xFFFFu) > density_threshold_u {
                                continue;
                            }

                            let jitter_x = f32((h >> 8u) & 0xFFu) / 255.0;
                            let jitter_z = f32((h >> 16u) & 0xFFu) / 255.0;

                            let blade_world_x = f32(gx) + jitter_x;
                            let blade_world_z = f32(gz) + jitter_z;
                            let rel_x = blade_world_x - chunk_min_x;
                            let rel_z = blade_world_z - chunk_min_z;
                            if rel_x < 0.0 || rel_x >= chunk_size || rel_z < 0.0 || rel_z >= chunk_size {
                                continue;
                            }

                            let blade_xz = vec2<f32>(blade_world_x, blade_world_z);
                            let t_plane = dot(blade_xz - ray_o_xz, blade_n_xz) * inv_denom;
                            if t_plane < tile_t_enter || t_plane >= best_t || t_plane >= tile_t_exit {
                                continue;
                            }

                            let ray_y_at_plane = ray_origin.y + ray_dir.y * t_plane;

                            // Pre-heightmap Y bounds check using tile range (register only)
                            if ray_y_at_plane < tile_min_y || ray_y_at_plane > tile_max_y_top {
                                if ENABLE_TRACE_STATS { trace_stats_grass_tile_rejects += 1u; }
                                continue;
                            }

                            let height_var = 0.7 + 0.3 * f32((h >> 24u) & 0xFFu) / 255.0;
                            let blade_h = GRASS_BLADE_HEIGHT * height_var;

                            var col_surface_y = default_surface_y;
                            if ENABLE_TRACE_STATS { trace_stats_grass_heightmap_reads += 1u; }
                            let local_x_u = u32(rel_x);
                            let local_z_u = u32(rel_z);
                            let col_idx = local_x_u + local_z_u * chunk_size_u;
                            let word = chunk_pool[bmp_offset + col_idx / 2u];
                            let height_off = (word >> ((col_idx % 2u) * 16u)) & 0xFFFFu;
                            if height_off == 0xFFFFu {
                                if ENABLE_TRACE_STATS { trace_stats_grass_column_misses += 1u; }
                                continue;
                            }
                            col_surface_y = foliage_base_y + f32(height_off) + 1.0;

                            if ENABLE_TRACE_STATS { trace_stats_grass_y_checks += 1u; }
                            let local_y = ray_y_at_plane - col_surface_y;
                            if local_y < 0.0 || local_y >= blade_h {
                                if ENABLE_TRACE_STATS { trace_stats_grass_y_rejects += 1u; }
                                continue;
                            }

                            let n_pixels = i32(ceil(blade_h / GRASS_PIXEL_SIZE));
                            let pixel_y = i32(floor(local_y / GRASS_PIXEL_SIZE));

                            let blade_width = GRASS_PIXEL_SIZE;

                            let phase = f32(h & 0xFFu) / 255.0 * 6.2831;
                            let height_frac = f32(pixel_y) / f32(max(n_pixels - 1, 1));

                            var sq_center_xz = blade_xz;
                            if t_plane < render_settings.vegetation_animation_distance {
                                // Per-blade frequency variation
                                let freq_var = 0.8 + 0.4 * f32((h >> 12u) & 0xFFu) / 255.0;
                                let blade_time = time_wind * freq_var;

                                // Spatial wind: coarse gust waves + fine turbulence
                                let coarse_travel = dot(blade_xz, wind_dir) * 0.05;
                                let fine_travel = dot(blade_xz, wind_dir) * 0.4;

                                // Gust envelope + local flutter (asymmetric: never sways backward)
                                let gust_envelope = max(sin(blade_time * 0.6 - coarse_travel), 0.0);
                                let flutter = sin(blade_time * 2.3 - fine_travel + phase);
                                let sway = GRASS_WIND_BASE_LEAN + gust_envelope * (0.5 + 0.5 * flutter);

                                // Cubic bend: stiffer base, flexible tip
                                let bend = height_frac * height_frac * height_frac;
                                let wind_factor = sway * bend * wind_bend_scale;
                                sq_center_xz = blade_xz + blade_t_xz * wind_factor;
                            }
                            let p_xz = ray_o_xz + ray_d_xz * t_plane;
                            let local_tang = dot(p_xz - sq_center_xz, blade_t_xz);
                            if abs(local_tang) > blade_width * 0.5 {
                                continue;
                            }

                            best_t = t_plane;
                            best_height_frac = height_frac;
                            best_color_var = 0.92 + 0.08 * f32((h >> 4u) & 0xFFu) / 255.0;
                        }
                    }
                }
            }
        }

        if !hit_x && !hit_z {
            break;
        }
        if hit_x {
            tile_x += step_x;
            t_next_x += t_delta_x;
        }
        if hit_z {
            tile_z += step_z;
            t_next_z += t_delta_z;
        }
        t_current = tile_t_exit;
    }

    if best_t < max_t {
        if ENABLE_TRACE_STATS { trace_stats_grass_trace_hits += 1u; }
        result.hit = true;
        result.t = best_t;
        result.pos = ray_origin + ray_dir * best_t;
        result.normal = vec3<f32>(0.0, 1.0, 0.0);

        let mat_color = render_settings.material_colors[GRASS_MATERIAL_ID].rgb;
        let ramp = mix(GRASS_COLOR_DARKEN, GRASS_COLOR_LIGHTEN, best_height_frac);
        result.color = min(mat_color * ramp, vec3<f32>(1.0)) * best_color_var;
    }

    return result;
}
