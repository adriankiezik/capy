// Chunk-level DDA: walks a ray through the voxel world grid one chunk at a time.
// Shared by all ray modes (primary, shadow, AO, reflection).

struct ChunkDDA {
    dir: vec3<f32>,
    t_current: f32,
    t_world_exit: f32,
    cc: vec3<i32>,
    t_delta: vec3<f32>,
    t_max: vec3<f32>,
    entry_axis: i32,
};

var<private> dda: ChunkDDA;

// Initialize the DDA for a ray. Returns false if the ray misses the world AABB.
// `t_min` skips the march to a known-safe start distance (0.0 = from the origin);
// the beam pre-pass supplies a conservative lower bound on the hit distance.
fn chunk_dda_init(ray_origin: vec3<f32>, ray_dir: vec3<f32>, t_min: f32) -> bool {
    let cs_xz = f32(streaming.chunk_size_xz);
    let cs_y = f32(streaming.chunk_size_y);

    var dir = ray_dir;
    let eps_d = 1e-10;
    if abs(dir.x) < eps_d { dir.x = select(-eps_d, eps_d, dir.x >= 0.0); }
    if abs(dir.y) < eps_d { dir.y = select(-eps_d, eps_d, dir.y >= 0.0); }
    if abs(dir.z) < eps_d { dir.z = select(-eps_d, eps_d, dir.z >= 0.0); }
    dda.dir = dir;

    let inv_dir = 1.0 / dir;
    let cs = vec3<f32>(cs_xz, cs_y, cs_xz);

    let world_min = vec3<f32>(
        f32(streaming.grid_min_x) * cs_xz,
        f32(streaming.grid_min_y) * cs_y,
        f32(streaming.grid_min_z) * cs_xz,
    );
    let world_max = vec3<f32>(
        f32(i32(streaming.grid_dim_x) + streaming.grid_min_x) * cs_xz,
        f32(i32(streaming.grid_dim_y) + streaming.grid_min_y) * cs_y,
        f32(i32(streaming.grid_dim_z) + streaming.grid_min_z) * cs_xz,
    );

    let t_world = intersect_aabb(ray_origin, dir, world_min, world_max);
    if t_world.x >= t_world.y || t_world.y <= 0.0 {
        return false;
    }

    dda.t_current = max(max(t_world.x, 0.0), t_min);
    dda.t_world_exit = t_world.y;
    if dda.t_current >= dda.t_world_exit {
        return false;
    }

    let ray_eps = max(render_settings.ray_epsilon, 0.0);
    let first_entry = ray_origin + dir * (dda.t_current + ray_eps);
    dda.cc = world_to_chunk_coord(
        clamp(first_entry, world_min + vec3<f32>(0.001), world_max - vec3<f32>(0.001)),
        cs_xz, cs_y,
    );

    dda.t_delta = abs(cs * inv_dir);

    dda.t_max = vec3<f32>(
        (f32(dda.cc.x + select(0, 1, dir.x > 0.0)) * cs_xz - ray_origin.x) * inv_dir.x,
        (f32(dda.cc.y + select(0, 1, dir.y > 0.0)) * cs_y - ray_origin.y) * inv_dir.y,
        (f32(dda.cc.z + select(0, 1, dir.z > 0.0)) * cs_xz - ray_origin.z) * inv_dir.z,
    );

    dda.entry_axis = -1;

    return true;
}

// Advance to the next chunk. Returns false when the ray exits the world.
fn chunk_dda_step() -> bool {
    if dda.t_max.x < dda.t_max.y && dda.t_max.x < dda.t_max.z {
        dda.entry_axis = 0;
        dda.t_current = dda.t_max.x;
        dda.cc.x += select(-1, 1, dda.dir.x > 0.0);
        dda.t_max.x += dda.t_delta.x;
    } else if dda.t_max.y < dda.t_max.z {
        dda.entry_axis = 1;
        dda.t_current = dda.t_max.y;
        dda.cc.y += select(-1, 1, dda.dir.y > 0.0);
        dda.t_max.y += dda.t_delta.y;
    } else {
        dda.entry_axis = 2;
        dda.t_current = dda.t_max.z;
        dda.cc.z += select(-1, 1, dda.dir.z > 0.0);
        dda.t_max.z += dda.t_delta.z;
    }

    if dda.cc.y < 0 { return false; }
    if dda.t_current >= dda.t_world_exit { return false; }
    return true;
}

// Ray t at current chunk entry (clamped to >= 0).
fn chunk_dda_t_enter() -> f32 {
    return max(dda.t_current, 0.0);
}

// Ray t at current chunk exit (next DDA boundary crossing).
fn chunk_dda_t_exit() -> f32 {
    return min(dda.t_max.x, min(dda.t_max.y, dda.t_max.z));
}

// World-space origin of the current chunk.
fn chunk_dda_chunk_min() -> vec3<f32> {
    return vec3<f32>(
        f32(dda.cc.x) * f32(streaming.chunk_size_xz),
        f32(dda.cc.y) * f32(streaming.chunk_size_y),
        f32(dda.cc.z) * f32(streaming.chunk_size_xz),
    );
}
