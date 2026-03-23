// ---------- Pixel Grass ----------
// 2D billboard grass blades on top of grass-material voxel surfaces.
// Each blade is a stack of independent pixel squares that face the camera.
// Wind moves each square independently — squares never deform, only slide sideways.
// Grass is traced per-chunk during DDA traversal, scoped to the chunk's
// foliage Y range, so blades correctly occlude (and are occluded by) solid voxels.

const GRASS_MATERIAL_ID: u32 = 1u;
const GRASS_BLADE_HEIGHT: f32 = 5.0;        // height in voxel units
const GRASS_PIXEL_SIZE: f32 = 0.5;          // size of each pixel square on the blade
const GRASS_BLADE_DENSITY: f32 = 0.9;       // probability a grid cell has a blade
const GRASS_MAX_DIST: f32 = 8000.0;          // distance beyond which grass is skipped

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
const GRASS_WIND_DIR: vec2<f32> = vec2<f32>(1.0, 0.6); // wind direction in XZ

// Blade color ramp: dark at base, lighter at tip
const GRASS_COLOR_BASE: vec3<f32> = vec3<f32>(0.12, 0.38, 0.08);
const GRASS_COLOR_MID: vec3<f32>  = vec3<f32>(0.20, 0.52, 0.15);
const GRASS_COLOR_TIP: vec3<f32>  = vec3<f32>(0.35, 0.65, 0.22);

fn grass_hash(x: i32, z: i32) -> u32 {
    var h = u32(x) * 374761393u + u32(z) * 668265263u;
    h = (h ^ (h >> 13u)) * 1274126177u;
    h = h ^ (h >> 16u);
    return h;
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
    trace_stats_grass_trace_calls += 1u;

    if foliage_y_bands == 0u { return result; }

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

    // Camera-facing blade orientation
    let cam_fwd_xz = vec2<f32>(ray_dir.x, ray_dir.z);
    if length(cam_fwd_xz) < 0.001 {
        return result; // looking straight up/down
    }
    let blade_normal_xz = -normalize(cam_fwd_xz);
    let blade_normal = vec3<f32>(blade_normal_xz.x, 0.0, blade_normal_xz.y);
    let blade_tangent = vec3<f32>(-blade_normal.z, 0.0, blade_normal.x);

    // Pre-compute denom (constant for all blades — blade_normal.y == 0).
    let denom = dot(ray_dir, blade_normal);
    if abs(denom) < 0.0001 {
        return result; // ray parallel to all blade planes
    }
    let inv_denom = 1.0 / denom;

    let cell_size = 1.0;
    let wind_dir = normalize(GRASS_WIND_DIR);

    // One XZ traversal for the chunk segment, clipped to the coarse foliage slab.
    let trace_t_start = max(max(t_enter_slab, chunk_t_enter), 0.0);
    let trace_t_end = min(min(t_exit_slab, chunk_t_exit), min(max_t, GRASS_MAX_DIST));
    if trace_t_start >= trace_t_end { return result; }

    let chunk_size_u = u32(chunk_size);

    var best_t = max_t;
    var best_color = vec3<f32>(0.0);
    var best_pos = vec3<f32>(0.0);

    // ── Fast path: uniform grass (all columns at default_surface_y) ──
    if bmp_offset == FOLIAGE_BMP_ALL {
        let p_start = ray_origin + ray_dir * trace_t_start;
        let p_end_raw = ray_origin + ray_dir * trace_t_end;
        let xz_span = length(vec2<f32>(p_end_raw.x - p_start.x, p_end_raw.z - p_start.z));
        let n_steps = max(i32(ceil(xz_span)), 1);
        let dt = (trace_t_end - trace_t_start) / f32(n_steps);

        for (var step = 0; step < n_steps; step++) {
            trace_stats_grass_steps += 1u;
            let t_sample = trace_t_start + (f32(step) + 0.5) * dt;
            if t_sample - 3.0 >= best_t { break; }

            let sample_pos = ray_origin + ray_dir * t_sample;
            let center_gx = i32(floor(sample_pos.x / cell_size));
            let center_gz = i32(floor(sample_pos.z / cell_size));

            var search_radius = 2;
            if t_sample >= 40.0 {
                search_radius = 1;
            }

            for (var dz = -search_radius; dz <= search_radius; dz++) {
                for (var dx = -search_radius; dx <= search_radius; dx++) {
                    trace_stats_grass_candidates += 1u;
                    let gx = center_gx + dx;
                    let gz = center_gz + dz;

                    let h = grass_hash(gx, gz);
                    if f32(h & 0xFFFFu) / 65535.0 > GRASS_BLADE_DENSITY {
                        continue;
                    }

                    let jitter_x = f32((h >> 8u) & 0xFFu) / 255.0;
                    let jitter_z = f32((h >> 16u) & 0xFFu) / 255.0;

                    let blade_world_x = (f32(gx) + jitter_x) * cell_size;
                    let blade_world_z = (f32(gz) + jitter_z) * cell_size;
                    let rel_x = blade_world_x - chunk_min_x;
                    let rel_z = blade_world_z - chunk_min_z;
                    if rel_x < 0.0 || rel_x >= chunk_size || rel_z < 0.0 || rel_z >= chunk_size {
                        continue;
                    }

                    let t_plane = ((blade_world_x - ray_origin.x) * blade_normal.x
                                 + (blade_world_z - ray_origin.z) * blade_normal.z) * inv_denom;
                    if t_plane < trace_t_start || t_plane >= best_t || t_plane >= trace_t_end {
                        continue;
                    }

                    let height_var = 0.3 + 0.7 * f32((h >> 24u) & 0xFFu) / 255.0;
                    let blade_h = GRASS_BLADE_HEIGHT * height_var;

                    let ray_y_at_plane = ray_origin.y + ray_dir.y * t_plane;
                    trace_stats_grass_y_checks += 1u;
                    let local_y = ray_y_at_plane - default_surface_y;
                    if local_y < 0.0 || local_y >= blade_h {
                        trace_stats_grass_y_rejects += 1u;
                        continue;
                    }

                    let n_pixels = i32(ceil(blade_h / GRASS_PIXEL_SIZE));
                    let pixel_y = i32(floor(local_y / GRASS_PIXEL_SIZE));
                    if pixel_y >= n_pixels {
                        continue;
                    }

                    let width_hash = (h >> 6u) & 0x3u;
                    let blade_width =
                        select(GRASS_PIXEL_SIZE, GRASS_PIXEL_SIZE * 2.0, width_hash == 0u);

                    let phase = f32(h & 0xFFu) / 255.0 * 6.2831;
                    let row_height_frac = f32(pixel_y) / f32(max(n_pixels - 1, 1));
                    let root_xz = vec2<f32>(blade_world_x, blade_world_z);

                    let travel = dot(root_xz, wind_dir) * 0.3;
                    let sway_main = sin(time * GRASS_WIND_SPEED - travel + phase);
                    let sway_detail =
                        sin(time * GRASS_WIND_SPEED * 2.7 - travel * 1.8 + phase * 2.1) * 0.3;
                    let gust = 0.5 + 0.5 * sin(time * 0.4 - travel * 0.1);
                    let sway_wind = (sway_main * 0.7 + sway_detail) * gust;

                    let cross_dir = vec2<f32>(-wind_dir.y, wind_dir.x);
                    let cross_travel = dot(root_xz, cross_dir) * 0.4;
                    let sway_cross =
                        sin(time * GRASS_WIND_SPEED * 1.3 - cross_travel + phase * 0.7)
                        * 0.2 * gust;

                    let bend = row_height_frac * row_height_frac;
                    let offset_wind =
                        blade_tangent * (sway_wind * bend * GRASS_WIND_STRENGTH * GRASS_PIXEL_SIZE);
                    let offset_cross =
                        blade_normal * (sway_cross * bend * GRASS_WIND_STRENGTH * GRASS_PIXEL_SIZE);
                    let square_offset = offset_wind + offset_cross;

                    let blade_root = vec3<f32>(blade_world_x, default_surface_y, blade_world_z);
                    let square_center = blade_root + square_offset;
                    let p = ray_origin + ray_dir * t_plane;
                    let local_tangent_x = dot(p - square_center, blade_tangent);
                    if abs(local_tangent_x) > blade_width * 0.5 {
                        continue;
                    }

                    let height_frac = f32(pixel_y) / f32(max(n_pixels - 1, 1));

                    var color: vec3<f32>;
                    if height_frac < 0.5 {
                        color = mix(GRASS_COLOR_BASE, GRASS_COLOR_MID, height_frac * 2.0);
                    } else {
                        color =
                            mix(GRASS_COLOR_MID, GRASS_COLOR_TIP, (height_frac - 0.5) * 2.0);
                    }

                    let color_var = 0.92 + 0.08 * f32((h >> 4u) & 0xFFu) / 255.0;
                    color = color * color_var;

                    best_t = t_plane;
                    best_color = color;
                    best_pos = p;
                }
            }
        }

        if best_t < max_t {
            trace_stats_grass_trace_hits += 1u;
            result.hit = true;
            result.t = best_t;
            result.color = best_color;
            result.pos = best_pos;
            result.normal = vec3<f32>(0.0, 1.0, 0.0);
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
                trace_stats_grass_run_visits += 1u;

                let p_start = ray_origin + ray_dir * tile_t_enter;
                let p_end_raw = ray_origin + ray_dir * tile_t_exit;
                let xz_span = length(vec2<f32>(p_end_raw.x - p_start.x, p_end_raw.z - p_start.z));
                let n_steps = max(i32(ceil(xz_span)), 1);
                let dt = (tile_t_exit - tile_t_enter) / f32(n_steps);

                for (var step = 0; step < n_steps; step++) {
                    trace_stats_grass_steps += 1u;
                    let t_sample = tile_t_enter + (f32(step) + 0.5) * dt;
                    if t_sample - 3.0 >= best_t { break; }

                    let sample_pos = ray_origin + ray_dir * t_sample;
                    let center_gx = i32(floor(sample_pos.x / cell_size));
                    let center_gz = i32(floor(sample_pos.z / cell_size));

                    var search_radius = 2;
                    if t_sample >= 40.0 {
                        search_radius = 1;
                    }

                    for (var dz = -search_radius; dz <= search_radius; dz++) {
                        for (var dx = -search_radius; dx <= search_radius; dx++) {
                            trace_stats_grass_candidates += 1u;
                            let gx = center_gx + dx;
                            let gz = center_gz + dz;

                            let h = grass_hash(gx, gz);
                            if f32(h & 0xFFFFu) / 65535.0 > GRASS_BLADE_DENSITY {
                                continue;
                            }

                            let jitter_x = f32((h >> 8u) & 0xFFu) / 255.0;
                            let jitter_z = f32((h >> 16u) & 0xFFu) / 255.0;

                            let blade_world_x = (f32(gx) + jitter_x) * cell_size;
                            let blade_world_z = (f32(gz) + jitter_z) * cell_size;
                            let rel_x = blade_world_x - chunk_min_x;
                            let rel_z = blade_world_z - chunk_min_z;
                            if rel_x < 0.0 || rel_x >= chunk_size || rel_z < 0.0 || rel_z >= chunk_size {
                                continue;
                            }

                            let t_plane = ((blade_world_x - ray_origin.x) * blade_normal.x
                                         + (blade_world_z - ray_origin.z) * blade_normal.z) * inv_denom;
                            if t_plane < tile_t_enter || t_plane >= best_t || t_plane >= tile_t_exit {
                                continue;
                            }

                            let ray_y_at_plane = ray_origin.y + ray_dir.y * t_plane;

                            // Pre-heightmap Y bounds check using tile range (register only)
                            if ray_y_at_plane < tile_min_y || ray_y_at_plane > tile_max_y_top {
                                trace_stats_grass_tile_rejects += 1u;
                                continue;
                            }

                            let height_var = 0.3 + 0.7 * f32((h >> 24u) & 0xFFu) / 255.0;
                            let blade_h = GRASS_BLADE_HEIGHT * height_var;

                            var col_surface_y = default_surface_y;
                            trace_stats_grass_heightmap_reads += 1u;
                            let local_x_u = u32(rel_x);
                            let local_z_u = u32(rel_z);
                            let col_idx = local_x_u + local_z_u * chunk_size_u;
                            let word = chunk_pool[bmp_offset + col_idx / 2u];
                            let height_off = (word >> ((col_idx % 2u) * 16u)) & 0xFFFFu;
                            if height_off == 0xFFFFu {
                                trace_stats_grass_column_misses += 1u;
                                continue;
                            }
                            col_surface_y = foliage_base_y + f32(height_off) + 1.0;

                            trace_stats_grass_y_checks += 1u;
                            let local_y = ray_y_at_plane - col_surface_y;
                            if local_y < 0.0 || local_y >= blade_h {
                                trace_stats_grass_y_rejects += 1u;
                                continue;
                            }

                            let n_pixels = i32(ceil(blade_h / GRASS_PIXEL_SIZE));
                            let pixel_y = i32(floor(local_y / GRASS_PIXEL_SIZE));
                            if pixel_y >= n_pixels {
                                continue;
                            }

                            let width_hash = (h >> 6u) & 0x3u;
                            let blade_width =
                                select(GRASS_PIXEL_SIZE, GRASS_PIXEL_SIZE * 2.0, width_hash == 0u);

                            let phase = f32(h & 0xFFu) / 255.0 * 6.2831;
                            let row_height_frac = f32(pixel_y) / f32(max(n_pixels - 1, 1));
                            let root_xz = vec2<f32>(blade_world_x, blade_world_z);

                            let travel = dot(root_xz, wind_dir) * 0.3;
                            let sway_main = sin(time * GRASS_WIND_SPEED - travel + phase);
                            let sway_detail =
                                sin(time * GRASS_WIND_SPEED * 2.7 - travel * 1.8 + phase * 2.1) * 0.3;
                            let gust = 0.5 + 0.5 * sin(time * 0.4 - travel * 0.1);
                            let sway_wind = (sway_main * 0.7 + sway_detail) * gust;

                            let cross_dir = vec2<f32>(-wind_dir.y, wind_dir.x);
                            let cross_travel = dot(root_xz, cross_dir) * 0.4;
                            let sway_cross =
                                sin(time * GRASS_WIND_SPEED * 1.3 - cross_travel + phase * 0.7)
                                * 0.2 * gust;

                            let bend = row_height_frac * row_height_frac;
                            let offset_wind =
                                blade_tangent * (sway_wind * bend * GRASS_WIND_STRENGTH * GRASS_PIXEL_SIZE);
                            let offset_cross =
                                blade_normal * (sway_cross * bend * GRASS_WIND_STRENGTH * GRASS_PIXEL_SIZE);
                            let square_offset = offset_wind + offset_cross;

                            let blade_root = vec3<f32>(blade_world_x, col_surface_y, blade_world_z);
                            let square_center = blade_root + square_offset;
                            let p = ray_origin + ray_dir * t_plane;
                            let local_x = dot(p - square_center, blade_tangent);
                            if abs(local_x) > blade_width * 0.5 {
                                continue;
                            }

                            let height_frac = f32(pixel_y) / f32(max(n_pixels - 1, 1));

                            var color: vec3<f32>;
                            if height_frac < 0.5 {
                                color = mix(GRASS_COLOR_BASE, GRASS_COLOR_MID, height_frac * 2.0);
                            } else {
                                color =
                                    mix(GRASS_COLOR_MID, GRASS_COLOR_TIP, (height_frac - 0.5) * 2.0);
                            }

                            let color_var = 0.92 + 0.08 * f32((h >> 4u) & 0xFFu) / 255.0;
                            color = color * color_var;

                            best_t = t_plane;
                            best_color = color;
                            best_pos = p;
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
        trace_stats_grass_trace_hits += 1u;
        result.hit = true;
        result.t = best_t;
        result.color = best_color;
        result.pos = best_pos;
        result.normal = vec3<f32>(0.0, 1.0, 0.0);
    }

    return result;
}
