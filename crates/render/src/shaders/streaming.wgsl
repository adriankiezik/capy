@group(0) @binding(0) var output: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(7) var depth_output: texture_storage_2d<r32float, write>;

struct StreamingInfo {
    grid_min_x: i32,
    grid_min_y: i32,
    grid_min_z: i32,
    grid_dim: u32,
    chunk_size: u32,
    pool_slot_count: u32,
    _pad0: u32,
    _pad1: u32,
};

struct SlotTreeInfo {
    world_size: u32,
    root_offset: u32,
    depth: u32,
    pool_offset: u32,
};

@group(0) @binding(1) var<uniform> camera: CameraUniform;
@group(0) @binding(2) var<uniform> streaming: StreamingInfo;
@group(0) @binding(3) var<storage, read> chunk_pool: array<u32>;
@group(0) @binding(4) var<storage, read> chunk_avg_pool: array<u32>;
@group(0) @binding(5) var<storage, read> indirection: array<u32>;
@group(0) @binding(6) var<storage, read_write> lod_debug_buf: array<u32>;
@group(0) @binding(8) var<uniform> render_settings: RenderSettingsUniform;

fn world_to_chunk_coord(world_pos: vec3<f32>, chunk_size: f32) -> vec3<i32> {
    return vec3<i32>(
        i32(floor(world_pos.x / chunk_size)),
        i32(floor(world_pos.y / chunk_size)),
        i32(floor(world_pos.z / chunk_size)),
    );
}

fn axis_normal(axis: i32, ray_dir: vec3<f32>) -> vec3<f32> {
    if axis == 0 {
        return vec3<f32>(select(1.0, -1.0, ray_dir.x >= 0.0), 0.0, 0.0);
    } else if axis == 1 {
        return vec3<f32>(0.0, select(1.0, -1.0, ray_dir.y >= 0.0), 0.0);
    } else if axis == 2 {
        return vec3<f32>(0.0, 0.0, select(1.0, -1.0, ray_dir.z >= 0.0));
    }
    return vec3<f32>(0.0, 1.0, 0.0);
}

fn lookup_chunk_info(cc: vec3<i32>) -> SlotTreeInfo {
    var info: SlotTreeInfo;
    info.world_size = 0u;

    let lx = cc.x - streaming.grid_min_x;
    let ly = cc.y - streaming.grid_min_y;
    let lz = cc.z - streaming.grid_min_z;
    let dim = i32(streaming.grid_dim);
    if lx < 0 || ly < 0 || lz < 0 || lx >= dim || ly >= dim || lz >= dim {
        return info;
    }
    let idx = u32(lx) + u32(ly) * streaming.grid_dim + u32(lz) * streaming.grid_dim * streaming.grid_dim;
    let base = idx * 4u;
    info.world_size = indirection[base];
    info.root_offset = indirection[base + 1u];
    info.depth = indirection[base + 2u];
    info.pool_offset = indirection[base + 3u];
    return info;
}

fn pool_read(pool_base: u32, offset: u32) -> u32 {
    return chunk_pool[pool_base + offset];
}

fn pool_read_avg(pool_base: u32, offset: u32) -> u32 {
    return chunk_avg_pool[pool_base + offset];
}

fn get_child_offset_pool(pool_base: u32, node_offset: u32, child_packed_idx: u32) -> u32 {
    return pool_read(pool_base, node_offset + 3u + child_packed_idx);
}

fn get_leaf_material_pool(pool_base: u32, node_offset: u32, bit: u32) -> u32 {
    let word_idx = bit / 4u;
    let byte_off = bit % 4u;
    let word = pool_read(pool_base, node_offset + 3u + word_idx);
    return (word >> (byte_off * 8u)) & 0xFFu;
}

fn get_node_avg_color_pool(pool_base: u32, node_offset: u32) -> vec3<f32> {
    let packed = pool_read_avg(pool_base, node_offset);
    let r = f32(packed & 0xFFu) / 255.0;
    let g = f32((packed >> 8u) & 0xFFu) / 255.0;
    let b = f32((packed >> 16u) & 0xFFu) / 255.0;
    return vec3<f32>(r, g, b);
}

fn get_cell_index(pos: vec3<f32>, scale_exp: u32) -> u32 {
    let ux = (bitcast<u32>(pos.x) >> scale_exp) & 3u;
    let uy = (bitcast<u32>(pos.y) >> scale_exp) & 3u;
    let uz = (bitcast<u32>(pos.z) >> scale_exp) & 3u;
    return ux + uy * 4u + uz * 16u;
}

fn floor_scale(pos: vec3<f32>, scale_exp: u32) -> vec3<f32> {
    let mask = ~0u << scale_exp;
    return vec3<f32>(
        bitcast<f32>(bitcast<u32>(pos.x) & mask),
        bitcast<f32>(bitcast<u32>(pos.y) & mask),
        bitcast<f32>(bitcast<u32>(pos.z) & mask),
    );
}

fn mirror_pos(pos: vec3<f32>, dir: vec3<f32>, range_check: bool) -> vec3<f32> {
    var mx = bitcast<f32>(bitcast<u32>(pos.x) ^ 0x7FFFFFu);
    var my = bitcast<f32>(bitcast<u32>(pos.y) ^ 0x7FFFFFu);
    var mz = bitcast<f32>(bitcast<u32>(pos.z) ^ 0x7FFFFFu);
    if range_check {
        if pos.x < 1.0 || pos.x >= 2.0 { mx = 3.0 - pos.x; }
        if pos.y < 1.0 || pos.y >= 2.0 { my = 3.0 - pos.y; }
        if pos.z < 1.0 || pos.z >= 2.0 { mz = 3.0 - pos.z; }
    }
    return vec3<f32>(
        select(pos.x, mx, dir.x > 0.0),
        select(pos.y, my, dir.y > 0.0),
        select(pos.z, mz, dir.z > 0.0),
    );
}

fn unmirror_pos(pos_m: vec3<f32>, dir: vec3<f32>) -> vec3<f32> {
    let ux = bitcast<f32>(bitcast<u32>(pos_m.x) ^ 0x7FFFFFu);
    let uy = bitcast<f32>(bitcast<u32>(pos_m.y) ^ 0x7FFFFFu);
    let uz = bitcast<f32>(bitcast<u32>(pos_m.z) ^ 0x7FFFFFu);
    return vec3<f32>(
        select(pos_m.x, ux, dir.x > 0.0),
        select(pos_m.y, uy, dir.y > 0.0),
        select(pos_m.z, uz, dir.z > 0.0),
    );
}

fn first_bit_high(x: u32) -> i32 {
    if x == 0u { return -1; }
    return i32(31u - countLeadingZeros(x));
}

struct StackEntry {
    node_idx: u32,
    mask_lo: u32,
    mask_hi: u32,
    is_leaf: bool,
};
var<private> stk: array<StackEntry, 12>;

struct HitResult {
    hit: bool,
    material: u32,
    normal: vec3<f32>,
    is_lod_hit: bool,
    color_override: vec3<f32>,
    lod_scale_exp: u32,
    hit_pos_local: vec3<f32>,
};

fn traverse_chunk(
    pool_base: u32,
    tree_info_ws: u32,
    tree_info_root: u32,
    tree_info_depth: u32,
    ray_origin_world: vec3<f32>,
    ray_dir_world: vec3<f32>,
    t_entry: f32,
    entry_axis: i32,
) -> HitResult {
    var result: HitResult;
    result.hit = false;
    result.is_lod_hit = false;
    result.lod_scale_exp = 0u;

    let ws = f32(tree_info_ws);
    let depth = tree_info_depth;
    let root_se = 21u;

    let dir = ray_dir_world;

    let origin_frac = ray_origin_world / ws + vec3<f32>(1.0);
    let entry_world = ray_origin_world + dir * t_entry;
    var pos = clamp(entry_world / ws + vec3<f32>(1.0), vec3<f32>(1.0), vec3<f32>(1.9999999));

    {
        var no = tree_info_root;
        var se = root_se;
        var ml = pool_read(pool_base, no);
        var mh = pool_read(pool_base, no + 1u);
        var il = (pool_read(pool_base, no + 2u) & 1u) != 0u;

        for (var d = 0u; d < depth; d++) {
            let ci = get_cell_index(pos, se);
            if il {
                if bit_is_set_64(ml, mh, ci) {
                    let mat = get_leaf_material_pool(pool_base, no, ci);
                    if mat != 0u {
                        result.hit = true;
                        result.material = mat;
                        result.hit_pos_local = (pos - vec3<f32>(1.0)) * ws;
                        result.normal = axis_normal(entry_axis, ray_dir_world);
                        return result;
                    }
                }
                break;
            }
            if !bit_is_set_64(ml, mh, ci) { break; }
            let pi = popcount_below(ml, mh, ci);
            no = get_child_offset_pool(pool_base, no, pi);
            ml = pool_read(pool_base, no);
            mh = pool_read(pool_base, no + 1u);
            il = (pool_read(pool_base, no + 2u) & 1u) != 0u;
            se -= 2u;
        }
    }

    var mirror_mask = 0u;
    if dir.x > 0.0 { mirror_mask |= 3u; }
    if dir.y > 0.0 { mirror_mask |= 3u << 2u; }
    if dir.z > 0.0 { mirror_mask |= 3u << 4u; }

    let origin_m = mirror_pos(origin_frac, dir, true);
    pos = mirror_pos(pos, dir, false);
    let inv_dir = 1.0 / -abs(dir);

    var node_idx = tree_info_root;
    var n_ml = pool_read(pool_base, node_idx);
    var n_mh = pool_read(pool_base, node_idx + 1u);
    var n_il = (pool_read(pool_base, node_idx + 2u) & 1u) != 0u;
    var scale_exp = root_se;
    var last_axis: i32 = -1;
    var side_dist = vec3<f32>(0.0);

    let max_node_steps = u32(max(round(render_settings.max_node_steps), 1.0));
    for (var i = 0u; i < max_node_steps; i++) {
        for (var dd = 0u; dd < depth; dd++) {
            let child_idx = get_cell_index(pos, scale_exp) ^ mirror_mask;

            if n_il || !bit_is_set_64(n_ml, n_mh, child_idx) { break; }

            if camera.lod_bias > 0.0 {
                let child_world_size = ws * exp2(f32(i32(scale_exp) - i32(root_se)));
                let pos_frac = unmirror_pos(pos, dir);
                let pos_world = (pos_frac - vec3<f32>(1.0)) * ws;
                let t = max(length(pos_world - ray_origin_world), 1.0);
                let projected = child_world_size / t;
                let threshold = camera.pixel_size * camera.lod_bias * render_settings.node_lod_scale;

                if projected < threshold {
                    let pi = popcount_below(n_ml, n_mh, child_idx);
                    let child_node_idx = get_child_offset_pool(pool_base, node_idx, pi);
                    let avg_color = get_node_avg_color_pool(pool_base, child_node_idx);

                    result.hit = true;
                    result.is_lod_hit = true;
                    result.color_override = avg_color;
                    result.normal = axis_normal(last_axis, ray_dir_world);
                    result.lod_scale_exp = scale_exp;
                    result.hit_pos_local = pos_world;
                    return result;
                }
            }

            stk[scale_exp >> 1u] = StackEntry(node_idx, n_ml, n_mh, n_il);

            let pi = popcount_below(n_ml, n_mh, child_idx);
            node_idx = get_child_offset_pool(pool_base, node_idx, pi);
            n_ml = pool_read(pool_base, node_idx);
            n_mh = pool_read(pool_base, node_idx + 1u);
            n_il = (pool_read(pool_base, node_idx + 2u) & 1u) != 0u;
            scale_exp -= 2u;
        }

        let child_idx = get_cell_index(pos, scale_exp) ^ mirror_mask;

        if n_il && bit_is_set_64(n_ml, n_mh, child_idx) {
            let mat = get_leaf_material_pool(pool_base, node_idx, child_idx);
            if mat != 0u {
                result.hit = true;
                result.material = mat;
                let dda_frac = unmirror_pos(pos, dir);
                result.hit_pos_local = (dda_frac - vec3<f32>(1.0)) * ws;
                result.normal = axis_normal(last_axis, ray_dir_world);
                return result;
            }
        }

        var adv_se = scale_exp;
        let aligned_ci = child_idx & 0x2Au;
        if aligned_ci < 32u {
            if ((n_ml >> aligned_ci) & 0x00330033u) == 0u { adv_se += 1u; }
        } else {
            if ((n_mh >> (aligned_ci - 32u)) & 0x00330033u) == 0u { adv_se += 1u; }
        }

        let cell_min = floor_scale(pos, adv_se);
        side_dist = (cell_min - origin_m) * inv_dir;

        var axis: i32;
        if side_dist.x < side_dist.y && side_dist.x < side_dist.z {
            axis = 0;
        } else if side_dist.y < side_dist.z {
            axis = 1;
        } else {
            axis = 2;
        }
        last_axis = axis;

        let tmax = select(select(side_dist.z, side_dist.y, axis == 1), side_dist.x, axis == 0);
        let one_ulp = i32((1u << adv_se) - 1u);

        var nb = vec3<i32>(
            bitcast<i32>(bitcast<u32>(cell_min.x)) + one_ulp,
            bitcast<i32>(bitcast<u32>(cell_min.y)) + one_ulp,
            bitcast<i32>(bitcast<u32>(cell_min.z)) + one_ulp,
        );
        if axis == 0 {
            nb.x = bitcast<i32>(bitcast<u32>(cell_min.x)) - 1;
        } else if axis == 1 {
            nb.y = bitcast<i32>(bitcast<u32>(cell_min.y)) - 1;
        } else {
            nb.z = bitcast<i32>(bitcast<u32>(cell_min.z)) - 1;
        }

        let advanced = origin_m - abs(dir) * tmax;
        pos = min(advanced, vec3<f32>(bitcast<f32>(nb.x), bitcast<f32>(nb.y), bitcast<f32>(nb.z)));

        let diff_bits = (bitcast<u32>(pos.x) ^ bitcast<u32>(cell_min.x))
                      | (bitcast<u32>(pos.y) ^ bitcast<u32>(cell_min.y))
                      | (bitcast<u32>(pos.z) ^ bitcast<u32>(cell_min.z));
        let diff_exp = first_bit_high(diff_bits & 0xFFAAAAAAu);

        if diff_exp > i32(scale_exp) {
            scale_exp = u32(diff_exp);
            if diff_exp > i32(root_se) {
                break;
            }
            let se = stk[scale_exp >> 1u];
            node_idx = se.node_idx;
            n_ml = se.mask_lo;
            n_mh = se.mask_hi;
            n_il = se.is_leaf;
        }
    }

    return result;
}

fn trace_ray(ray_origin: vec3<f32>, ray_dir: vec3<f32>) -> HitResult {
    var result: HitResult;
    result.hit = false;
    result.is_lod_hit = false;
    result.lod_scale_exp = 0u;

    let cs = f32(streaming.chunk_size);
    var dir = ray_dir;
    let eps_d = 1e-10;
    if abs(dir.x) < eps_d { dir.x = select(-eps_d, eps_d, dir.x >= 0.0); }
    if abs(dir.y) < eps_d { dir.y = select(-eps_d, eps_d, dir.y >= 0.0); }
    if abs(dir.z) < eps_d { dir.z = select(-eps_d, eps_d, dir.z >= 0.0); }

    let inv_dir = 1.0 / dir;

    let grid_dim = i32(streaming.grid_dim);
    let world_min = vec3<f32>(
        f32(streaming.grid_min_x) * cs,
        f32(streaming.grid_min_y) * cs,
        f32(streaming.grid_min_z) * cs,
    );
    let world_max = vec3<f32>(
        f32(streaming.grid_min_x + grid_dim) * cs,
        f32(streaming.grid_min_y + grid_dim) * cs,
        f32(streaming.grid_min_z + grid_dim) * cs,
    );

    let t_world = intersect_aabb(ray_origin, dir, world_min, world_max);
    if t_world.x >= t_world.y || t_world.y <= 0.0 {
        return result;
    }

    var t_current = max(t_world.x, 0.0);

    let ray_eps = max(render_settings.ray_epsilon, 0.0);
    let first_entry = ray_origin + dir * (t_current + ray_eps);
    var cc = world_to_chunk_coord(
        clamp(first_entry, world_min + vec3<f32>(0.001), world_max - vec3<f32>(0.001)),
        cs,
    );

    let step = vec3<i32>(
        select(-1, 1, dir.x > 0.0),
        select(-1, 1, dir.y > 0.0),
        select(-1, 1, dir.z > 0.0),
    );
    let t_delta = abs(vec3<f32>(cs) * inv_dir);

    var t_max = vec3<f32>(
        (f32(cc.x + select(0, 1, dir.x > 0.0)) * cs - ray_origin.x) * inv_dir.x,
        (f32(cc.y + select(0, 1, dir.y > 0.0)) * cs - ray_origin.y) * inv_dir.y,
        (f32(cc.z + select(0, 1, dir.z > 0.0)) * cs - ray_origin.z) * inv_dir.z,
    );

    var entry_axis: i32 = -1;

    let max_chunk_steps = u32(max(round(render_settings.max_chunk_steps), 1.0));
    for (var chunk_iter = 0u; chunk_iter < max_chunk_steps; chunk_iter++) {
        let info = lookup_chunk_info(cc);

        if info.world_size != 0u {

            if camera.lod_bias > 0.0 {
                let chunk_center = vec3<f32>(
                    (f32(cc.x) + 0.5) * cs,
                    (f32(cc.y) + 0.5) * cs,
                    (f32(cc.z) + 0.5) * cs,
                );
                let dist = max(length(chunk_center - ray_origin), 1.0);
                let projected = cs / dist;
                if projected < camera.pixel_size * camera.lod_bias * render_settings.chunk_lod_scale {
                    let pool_base = info.pool_offset;
                    let avg_color = get_node_avg_color_pool(pool_base, info.root_offset);
                    if avg_color.x > 0.001 || avg_color.y > 0.001 || avg_color.z > 0.001 {
                        result.hit = true;
                        result.is_lod_hit = true;
                        result.color_override = avg_color;
                        result.lod_scale_exp = 23u;
                        result.hit_pos_local = ray_origin + dir * max(t_current, 0.0);
                        result.normal = axis_normal(entry_axis, ray_dir);
                        return result;
                    }
                }
            }

            let pool_base = info.pool_offset;
            let chunk_min = vec3<f32>(f32(cc.x) * cs, f32(cc.y) * cs, f32(cc.z) * cs);
            let local_origin = ray_origin - chunk_min;

            let chunk_hit = traverse_chunk(
                pool_base,
                info.world_size,
                info.root_offset,
                info.depth,
                local_origin,
                dir,
                max(t_current, 0.0),
                entry_axis,
            );

            if chunk_hit.hit {
                var world_hit = chunk_hit;
                world_hit.hit_pos_local = chunk_hit.hit_pos_local + chunk_min;
                return world_hit;
            }
        }

        if t_max.x < t_max.y && t_max.x < t_max.z {
            entry_axis = 0;
            t_current = t_max.x;
            cc.x += step.x;
            t_max.x += t_delta.x;
        } else if t_max.y < t_max.z {
            entry_axis = 1;
            t_current = t_max.y;
            cc.y += step.y;
            t_max.y += t_delta.y;
        } else {
            entry_axis = 2;
            t_current = t_max.z;
            cc.z += step.z;
            t_max.z += t_delta.z;
        }

        if t_current >= t_world.y {
            break;
        }
    }

    return result;
}

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let dims = textureDimensions(output);

    let actual_x = gid.x;
    let actual_y = gid.y;
    if actual_x >= dims.x || actual_y >= dims.y { return; }

    let uv_x = (f32(actual_x) + 0.5) / camera.resolution.x;
    let uv_y = 1.0 - (f32(actual_y) + 0.5) / camera.resolution.y;

    let ray_dir = normalize(
        camera.ray_corner
        + camera.ray_right * (uv_x * 2.0)
        + camera.ray_up * (uv_y * 2.0)
    );
    let ray_origin = camera.camera_pos;

    let hit = trace_ray(ray_origin, ray_dir);

    let pixel_idx = actual_y * u32(camera.resolution.x) + actual_x;
    if pixel_idx < arrayLength(&lod_debug_buf) {
        lod_debug_buf[pixel_idx] = hit.lod_scale_exp;
    }

    let sun_dir = normalize(render_settings.sun_direction.xyz);
    let pixel = vec2<i32>(i32(actual_x), i32(actual_y));
    if hit.hit {
        let n_dot_l = max(dot(hit.normal, sun_dir), 0.0);
        let light = render_settings.ambient_light + render_settings.sun_contribution * n_dot_l;
        var base: vec3<f32>;
        if hit.is_lod_hit {
            base = hit.color_override;
        } else {
            base = render_settings.material_colors[min(hit.material, 7u)].rgb;
        }
        textureStore(output, pixel, vec4<f32>(base * light, 1.0));
        let depth_val = length(hit.hit_pos_local - ray_origin);
        textureStore(depth_output, pixel, vec4<f32>(depth_val, 0.0, 0.0, 0.0));
    } else {
        textureStore(output, pixel, vec4<f32>(render_settings.sky_color.rgb, 1.0));
        textureStore(depth_output, pixel, vec4<f32>(0.0, 0.0, 0.0, 0.0));
    }
}
