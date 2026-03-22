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

// Trace grass blades within a bounded Y slab [foliage_base_y, slab_top_y],
// clamped to the chunk's ray segment [chunk_t_enter, chunk_t_exit].
// Called from the DDA loop per chunk that has foliage.
//
// foliage_base_y = chunk_min.y + foliage_y_min (world-space base for heightmap offsets).
// slab_top_y     = chunk_min.y + foliage_y_max + GRASS_BLADE_HEIGHT.
// For FOLIAGE_BMP_ALL (all columns at same height), blade root = foliage_base_y + 1.
// Otherwise, the heightmap stores per-column u8 offsets from foliage_y_min;
// blade root = foliage_base_y + offset + 1.
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
    foliage_y_bands: u32, // 32-bit Y-occupancy mask (bit i = foliage in [i*32, (i+1)*32))
) -> GrassHit {
    var result: GrassHit;
    result.hit = false;
    result.t = 1e20;

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

    // Clamp march range: intersection of Y slab, chunk bounds, and max_t
    let t_start = max(max(t_enter_slab, chunk_t_enter), 0.0);
    let t_end = min(min(t_exit_slab, chunk_t_exit), max_t);
    if t_start >= t_end { return result; }

    // Distance cull
    if t_start > GRASS_MAX_DIST { return result; }

    // Cap the march distance through the slab to avoid artifacts and perf issues
    // when the ray skims the grass slab horizontally (side views).
    let max_march = min(t_end, t_start + 64.0);

    // Determine how many march steps we need based on XZ span.
    // Step size ≈ 1 cell ensures the search neighborhood covers all cells with no gaps.
    let p_start = ray_origin + ray_dir * t_start;
    let p_end_raw = ray_origin + ray_dir * max_march;
    let xz_span = length(vec2<f32>(p_end_raw.x - p_start.x, p_end_raw.z - p_start.z));
    let n_steps = clamp(i32(ceil(xz_span)), 1, 64);
    let dt = (max_march - t_start) / f32(n_steps);

    var best_t = max_t;
    var best_color = vec3<f32>(0.0);
    var best_pos = vec3<f32>(0.0);

    for (var step = 0; step < n_steps; step++) {
        let t_sample = t_start + (f32(step) + 0.5) * dt;
        if t_sample - 3.0 >= best_t { break; }

        let sample_pos = ray_origin + ray_dir * t_sample;

        // Y-band early exit: skip this step entirely if no foliage exists
        // in the 32-voxel Y band containing the ray's current position.
        // This avoids expensive per-cell work when the ray passes through
        // empty vertical space between distinct grass surfaces (e.g. ground
        // grass at Y=128 and building-roof grass at Y=300).
        let local_y_band = sample_pos.y - chunk_min_y;
        if local_y_band >= 0.0 && local_y_band < 1024.0 {
            let band = u32(local_y_band) >> 5u; // / 32
            if (foliage_y_bands & (1u << band)) == 0u {
                continue;
            }
        }

        let center_gx = i32(floor(sample_pos.x / cell_size));
        let center_gz = i32(floor(sample_pos.z / cell_size));

        // Distance-based LOD on the neighborhood radius
        var search_radius = 1;
        if t_sample < 40.0 {
            search_radius = 2;
        }

    for (var dz = -search_radius; dz <= search_radius; dz++) {
        for (var dx = -search_radius; dx <= search_radius; dx++) {
            let gx = center_gx + dx;
            let gz = center_gz + dz;

            let h = grass_hash(gx, gz);

            if f32(h & 0xFFFFu) / 65535.0 > GRASS_BLADE_DENSITY {
                continue;
            }

            let jitter_x = f32((h >> 8u) & 0xFFu) / 255.0;
            let jitter_z = f32((h >> 16u) & 0xFFu) / 255.0;

            // Skip blades whose root falls outside this chunk — the neighboring
            // chunk's DDA pass will trace them with its own heightmap.
            let blade_world_x = (f32(gx) + jitter_x) * cell_size;
            let blade_world_z = (f32(gz) + jitter_z) * cell_size;
            let rel_x = blade_world_x - chunk_min_x;
            let rel_z = blade_world_z - chunk_min_z;
            if rel_x < 0.0 || rel_x >= chunk_size || rel_z < 0.0 || rel_z >= chunk_size {
                continue;
            }

            // Compute t_plane early (blade_normal.y == 0, so this is XZ-only
            // and does NOT depend on the heightmap surface Y).
            let t_plane = ((blade_world_x - ray_origin.x) * blade_normal.x
                         + (blade_world_z - ray_origin.z) * blade_normal.z) * inv_denom;
            if t_plane < 0.0 || t_plane >= best_t {
                continue;
            }

            // Blade height from hash (cheap, no memory read)
            let height_var = 0.3 + 0.7 * f32((h >> 24u) & 0xFFu) / 255.0;
            let blade_h = GRASS_BLADE_HEIGHT * height_var;

            // Ray Y at the blade plane — used for the Y range check after
            // reading the heightmap.
            let ray_y_at_plane = ray_origin.y + ray_dir.y * t_plane;

            // Look up the per-column foliage surface Y from the heightmap.
            // FOLIAGE_BMP_ALL: all columns at default_surface_y (single height).
            // Otherwise: read a u8 offset per column (0xFF = no foliage).
            var col_surface_y = default_surface_y;
            if bmp_offset != FOLIAGE_BMP_ALL {
                let local_x = u32(rel_x);
                let local_z = u32(rel_z);
                let col_idx = local_x + local_z * u32(chunk_size);
                let word = chunk_pool[bmp_offset + col_idx / 4u];
                let byte_val = (word >> ((col_idx % 4u) * 8u)) & 0xFFu;
                if byte_val == 0xFFu {
                    continue; // no foliage in this column
                }
                col_surface_y = foliage_base_y + f32(byte_val) + 1.0;
            }

            // Y range check: blade extends from col_surface_y to col_surface_y + blade_h
            let local_y = ray_y_at_plane - col_surface_y;
            if local_y < 0.0 || local_y >= blade_h {
                continue;
            }

            let n_pixels = i32(ceil(blade_h / GRASS_PIXEL_SIZE));
            let pixel_y = i32(floor(local_y / GRASS_PIXEL_SIZE));
            if pixel_y >= n_pixels {
                continue;
            }

            // Width: most blades 1 pixel, ~25% are 2 pixels wide
            let width_hash = (h >> 6u) & 0x3u;
            let blade_width = select(GRASS_PIXEL_SIZE, GRASS_PIXEL_SIZE * 2.0, width_hash == 0u);

            let phase = f32(h & 0xFFu) / 255.0 * 6.2831;

            // Per-square wind offset
            let row_height_frac = f32(pixel_y) / f32(max(n_pixels - 1, 1));
            let root_xz = vec2<f32>(blade_world_x, blade_world_z);

            // Traveling wave: wind rolls across the field spatially
            let travel = dot(root_xz, wind_dir) * 0.3;
            // Main sway: large slow wave
            let sway_main = sin(time * GRASS_WIND_SPEED - travel + phase);
            // Detail rustle: faster, smaller, different spatial frequency
            let sway_detail = sin(time * GRASS_WIND_SPEED * 2.7 - travel * 1.8 + phase * 2.1) * 0.3;
            // Gust: slow amplitude modulation — wind comes and goes
            let gust = 0.5 + 0.5 * sin(time * 0.4 - travel * 0.1);
            // Combined sway along wind direction
            let sway_wind = (sway_main * 0.7 + sway_detail) * gust;

            // Cross-wind: slight perpendicular turbulence
            let cross_dir = vec2<f32>(-wind_dir.y, wind_dir.x);
            let cross_travel = dot(root_xz, cross_dir) * 0.4;
            let sway_cross = sin(time * GRASS_WIND_SPEED * 1.3 - cross_travel + phase * 0.7) * 0.2 * gust;

            let bend = row_height_frac * row_height_frac;
            let offset_wind = blade_tangent * (sway_wind * bend * GRASS_WIND_STRENGTH * GRASS_PIXEL_SIZE);
            // Cross-wind offset along blade normal (pushes blade forward/back)
            let offset_cross = blade_normal * (sway_cross * bend * GRASS_WIND_STRENGTH * GRASS_PIXEL_SIZE);
            let square_offset = offset_wind + offset_cross;

            let blade_root = vec3<f32>(blade_world_x, col_surface_y, blade_world_z);
            let square_center = blade_root + square_offset;
            let p = ray_origin + ray_dir * t_plane;
            let local_x = dot(p - square_center, blade_tangent);
            if abs(local_x) > blade_width * 0.5 {
                continue;
            }

            // Color: per-blade variety derived from hash
            let height_frac = f32(pixel_y) / f32(max(n_pixels - 1, 1));

            var color: vec3<f32>;
            if height_frac < 0.5 {
                color = mix(GRASS_COLOR_BASE, GRASS_COLOR_MID, height_frac * 2.0);
            } else {
                color = mix(GRASS_COLOR_MID, GRASS_COLOR_TIP, (height_frac - 0.5) * 2.0);
            }

            // Very subtle per-blade brightness variation
            let color_var = 0.92 + 0.08 * f32((h >> 4u) & 0xFFu) / 255.0;
            color = color * color_var;

            best_t = t_plane;
            best_color = color;
            best_pos = p;
        }
    }
    } // end step loop

    if best_t < max_t {
        result.hit = true;
        result.t = best_t;
        result.color = best_color;
        result.pos = best_pos;
        result.normal = vec3<f32>(0.0, 1.0, 0.0);
    }

    return result;
}
