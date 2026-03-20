// ---------- Pixel Grass ----------
// 2D billboard grass blades on top of grass-material voxel surfaces.
// Each blade is a stack of independent pixel squares that face the camera.
// Wind moves each square independently — squares never deform, only slide sideways.
// Grass is traced independently from the voxel DAG so blades correctly
// occlude (and are occluded by) neighboring solid voxels.

const GRASS_MATERIAL_ID: u32 = 1u;
const GRASS_BLADE_HEIGHT: f32 = 5.0;        // height in voxel units
const GRASS_PIXEL_SIZE: f32 = 0.5;          // size of each pixel square on the blade
const GRASS_BLADE_DENSITY: f32 = 0.9;       // probability a grid cell has a blade
const GRASS_SEARCH_RADIUS: i32 = 4;         // how many grid cells to check around hit
const GRASS_MAX_DIST: f32 = 8000.0;          // distance beyond which grass is skipped
const GRASS_SURFACE_Y: f32 = 128.0;         // Y level of the grass surface (flat terrain top)

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

// Trace grass blades independently along the ray.
// Tests all blade intersections in the grass slab [GRASS_SURFACE_Y, GRASS_SURFACE_Y + GRASS_BLADE_HEIGHT]
// and returns the closest hit. This runs before/alongside the voxel trace so grass
// can appear in front of solid voxels.
fn trace_grass_ray(
    ray_origin: vec3<f32>,
    ray_dir: vec3<f32>,
    time: f32,
    max_t: f32,           // reject any grass hit beyond this t (voxel hit distance)
) -> GrassHit {
    var result: GrassHit;
    result.hit = false;
    result.t = 1e20;

    let surface_y = GRASS_SURFACE_Y;
    let top_y = surface_y + GRASS_BLADE_HEIGHT;

    // Find where the ray intersects the grass Y slab
    let inv_dy = 1.0 / ray_dir.y;
    let t_bottom = (surface_y - ray_origin.y) * inv_dy;
    let t_top = (top_y - ray_origin.y) * inv_dy;
    let t_enter = max(min(t_bottom, t_top), 0.0);
    let t_exit = max(t_bottom, t_top);

    if t_exit < 0.0 || t_enter > t_exit {
        return result;
    }

    // Distance cull
    if t_enter > GRASS_MAX_DIST {
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

    // Search center: where the ray is at the middle of the slab
    let t_mid = (t_enter + min(t_exit, t_enter + 20.0)) * 0.5;
    let mid_pos = ray_origin + ray_dir * t_mid;
    let cell_size = 1.0;
    let center_gx = i32(floor(mid_pos.x / cell_size));
    let center_gz = i32(floor(mid_pos.z / cell_size));

    let wind_dir = normalize(GRASS_WIND_DIR);

    // Distance-based LOD: reduce search radius at distance
    let grass_dist = t_enter;
    var search_radius = GRASS_SEARCH_RADIUS;
    if grass_dist > 2000.0 {
        search_radius = 1;
    } else if grass_dist > 1000.0 {
        search_radius = 2;
    }

    var best_t = max_t;
    var best_color = vec3<f32>(0.0);
    var best_pos = vec3<f32>(0.0);

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
            let blade_root = vec3<f32>(
                (f32(gx) + jitter_x) * cell_size,
                surface_y,
                (f32(gz) + jitter_z) * cell_size,
            );

            // Height: wider range — short stubble to tall blades
            let height_var = 0.3 + 0.7 * f32((h >> 24u) & 0xFFu) / 255.0;
            let blade_h = GRASS_BLADE_HEIGHT * height_var;
            let n_pixels = i32(ceil(blade_h / GRASS_PIXEL_SIZE));

            // Width: most blades 1 pixel, ~25% are 2 pixels wide
            let width_hash = (h >> 6u) & 0x3u;
            let blade_width = select(GRASS_PIXEL_SIZE, GRASS_PIXEL_SIZE * 2.0, width_hash == 0u);

            let phase = f32(h & 0xFFu) / 255.0 * 6.2831;

            // Ray-plane intersection (blade plane through root, facing camera)
            let denom = dot(ray_dir, blade_normal);
            if abs(denom) < 0.0001 {
                continue;
            }
            let t_plane = dot(blade_root - ray_origin, blade_normal) / denom;
            if t_plane < 0.0 || t_plane >= best_t {
                continue;
            }

            let p = ray_origin + ray_dir * t_plane;
            let local_y = p.y - surface_y;
            if local_y < 0.0 || local_y >= blade_h {
                continue;
            }

            let pixel_y = i32(floor(local_y / GRASS_PIXEL_SIZE));
            if pixel_y >= n_pixels {
                continue;
            }

            // Per-square wind offset
            let row_height_frac = f32(pixel_y) / f32(max(n_pixels - 1, 1));
            let root_xz = vec2<f32>(blade_root.x, blade_root.z);

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

            let square_center = blade_root + square_offset;
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

    if best_t < max_t {
        result.hit = true;
        result.t = best_t;
        result.color = best_color;
        result.pos = best_pos;
        result.normal = vec3<f32>(0.0, 1.0, 0.0);
    }

    return result;
}
