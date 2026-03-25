struct StreamingInfo {
    grid_min_x: i32,
    grid_min_y: i32,
    grid_min_z: i32,
    grid_dim_x: u32,
    grid_dim_y: u32,
    grid_dim_z: u32,
    chunk_size_xz: u32,
    chunk_size_y: u32,
};

struct SlotTreeInfo {
    world_size: u32,
    root_offset: u32,
    depth: u32,
    pool_offset: u32,
    foliage_y_min: u32,
    foliage_y_max: u32,
    foliage_bitmap_offset: u32,
    foliage_y_bands: u32,
    foliage_tile_y_ranges_offset: u32,
};

fn world_to_chunk_coord(world_pos: vec3<f32>, cs_xz: f32, cs_y: f32) -> vec3<i32> {
    return vec3<i32>(
        i32(floor(world_pos.x / cs_xz)),
        i32(floor(world_pos.y / cs_y)),
        i32(floor(world_pos.z / cs_xz)),
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
    if lx < 0 || ly < 0 || lz < 0
        || lx >= i32(streaming.grid_dim_x)
        || ly >= i32(streaming.grid_dim_y)
        || lz >= i32(streaming.grid_dim_z) {
        return info;
    }
    let idx = u32(lx) + u32(ly) * streaming.grid_dim_x + u32(lz) * streaming.grid_dim_x * streaming.grid_dim_y;
    let base = idx * 9u;
    info.world_size = indirection[base];
    info.root_offset = indirection[base + 1u];
    info.depth = indirection[base + 2u];
    info.pool_offset = indirection[base + 3u];
    info.foliage_y_min = indirection[base + 4u];
    info.foliage_y_max = indirection[base + 5u];
    info.foliage_bitmap_offset = indirection[base + 6u];
    info.foliage_y_bands = indirection[base + 7u];
    info.foliage_tile_y_ranges_offset = indirection[base + 8u];
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
    let word_idx = bit / 2u;
    let half_off = bit % 2u;
    let word = pool_read(pool_base, node_offset + 3u + word_idx);
    return (word >> (half_off * 16u)) & 0xFFFFu;
}

const NODE_FLAG_LEAF: u32 = 0x1u;
const NODE_FLAG_UNIFORM_WATER: u32 = 0x2u;

fn get_node_flags_pool(pool_base: u32, node_offset: u32) -> u32 {
    return pool_read(pool_base, node_offset + 2u);
}

fn node_is_leaf(flags: u32) -> bool {
    return (flags & NODE_FLAG_LEAF) != 0u;
}

fn node_is_uniform_water(flags: u32) -> bool {
    return (flags & NODE_FLAG_UNIFORM_WATER) != 0u;
}

fn get_node_avg_color_pool(pool_base: u32, node_offset: u32) -> vec3<f32> {
    let packed = pool_read_avg(pool_base, node_offset);
    let r = f32(packed & 0xFFu) / 255.0;
    let g = f32((packed >> 8u) & 0xFFu) / 255.0;
    let b = f32((packed >> 16u) & 0xFFu) / 255.0;
    return vec3<f32>(r, g, b);
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
// Best grass hit found during DDA traversal, read by trace.wgsl after trace_ray().
var<private> dda_grass_hit: GrassHit;
// When true, trace_ray() ignores grass entirely (used by pick shader for editor tools).
var<private> skip_grass: bool;

// Water voxel hit found during DDA traversal, read by trace.wgsl after trace_ray().
// The traversal skips water voxels (treats them as transparent) and records the
// closest water surface hit here. The solid hit behind water (if any) is returned
// normally as the HitResult from trace_ray().
const WATER_BIT_MASK: u32 = 0x4000u;

struct WaterHit {
    hit: bool,
    t: f32,                   // ray parameter at water surface (world-space distance)
    entry_normal: vec3<f32>,  // flat face normal from entry axis (before wave perturbation)
};
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

struct HitResult {
    hit: bool,
    material: u32,
    normal: vec3<f32>,
    is_lod_hit: bool,
    color_override: vec3<f32>,
    lod_scale_exp: u32,
    hit_pos_local: vec3<f32>,
    t: f32,              // ray parameter (world-space distance, since dir is normalized)
};

// Point query: check if a world-space position contains a solid voxel.
fn is_voxel_solid(world_pos: vec3<f32>) -> bool {
    let cs_xz = f32(streaming.chunk_size_xz);
    let cs_y = f32(streaming.chunk_size_y);
    let cc = world_to_chunk_coord(world_pos, cs_xz, cs_y);
    let info = lookup_chunk_info(cc);
    if info.world_size == 0u {
        return false;
    }

    let ws = f32(info.world_size);
    let chunk_min = vec3<f32>(f32(cc.x) * cs_xz, f32(cc.y) * cs_y, f32(cc.z) * cs_xz);
    let local = world_pos - chunk_min;
    let pos = clamp(local / ws + vec3<f32>(1.0), vec3<f32>(1.0), vec3<f32>(1.9999999));

    let pool_base = info.pool_offset;
    var no = info.root_offset;
    var se = 21u;
    var ml = pool_read(pool_base, no);
    var mh = pool_read(pool_base, no + 1u);
    var flags = get_node_flags_pool(pool_base, no);
    if node_is_uniform_water(flags) {
        return true;
    }
    var il = node_is_leaf(flags);

    for (var d = 0u; d < info.depth; d++) {
        let ci = get_cell_index(pos, se);
        if il {
            if bit_is_set_64(ml, mh, ci) {
                let mat = get_leaf_material_pool(pool_base, no, ci);
                return mat != 0u;
            }
            return false;
        }
        if !bit_is_set_64(ml, mh, ci) {
            return false;
        }
        let pi = popcount_below(ml, mh, ci);
        let child_no = get_child_offset_pool(pool_base, no, pi);
        let child_flags = get_node_flags_pool(pool_base, child_no);
        if node_is_uniform_water(child_flags) {
            return true;
        }
        no = child_no;
        ml = pool_read(pool_base, no);
        mh = pool_read(pool_base, no + 1u);
        flags = child_flags;
        il = node_is_leaf(flags);
        se -= 2u;
    }
    return false;
}

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
    let root_flags = get_node_flags_pool(pool_base, tree_info_root);

    if node_is_uniform_water(root_flags) {
        record_water_surface_hit(
            ray_origin_world,
            ray_dir_world,
            (pos - vec3<f32>(1.0)) * ws,
            entry_axis,
        );
        return result;
    }

    {
        var no = tree_info_root;
        var se = root_se;
        var ml = pool_read(pool_base, no);
        var mh = pool_read(pool_base, no + 1u);
        var flags = root_flags;
        var il = node_is_leaf(flags);

        for (var d = 0u; d < depth; d++) {
            let ci = get_cell_index(pos, se);
            if il {
                if bit_is_set_64(ml, mh, ci) {
                    let mat = get_leaf_material_pool(pool_base, no, ci);
                    if mat != 0u {
                        if (mat & WATER_BIT_MASK) != 0u {
                            // Water voxel at entry — record surface and fall through to DDA
                            record_water_surface_hit(
                                ray_origin_world,
                                ray_dir_world,
                                (pos - vec3<f32>(1.0)) * ws,
                                entry_axis,
                            );
                            // Water disabled: treat as air — skip this voxel
                        } else {
                            result.hit = true;
                            result.material = mat;
                            result.hit_pos_local = (pos - vec3<f32>(1.0)) * ws;
                            result.t = dot(result.hit_pos_local - ray_origin_world, dir);
                            result.normal = axis_normal(entry_axis, ray_dir_world);
                            return result;
                        }
                    }
                }
                break;
            }
            if !bit_is_set_64(ml, mh, ci) { break; }
            let pi = popcount_below(ml, mh, ci);
            let child_no = get_child_offset_pool(pool_base, no, pi);
            let child_flags = get_node_flags_pool(pool_base, child_no);
            if node_is_uniform_water(child_flags) {
                record_water_surface_hit(
                    ray_origin_world,
                    ray_dir_world,
                    (pos - vec3<f32>(1.0)) * ws,
                    entry_axis,
                );
                break;
            }
            no = child_no;
            ml = pool_read(pool_base, no);
            mh = pool_read(pool_base, no + 1u);
            flags = child_flags;
            il = node_is_leaf(flags);
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
    var n_flags = root_flags;
    var n_il = node_is_leaf(n_flags);
    var scale_exp = root_se;
    var last_axis: i32 = -1;
    var side_dist = vec3<f32>(0.0);

    let max_node_steps = u32(max(round(render_settings.max_node_steps), 1.0));
    for (var i = 0u; i < max_node_steps; i++) {
        if ENABLE_TRACE_STATS { trace_stats_primary_node_steps += 1u; }
        for (var dd = 0u; dd < depth; dd++) {
            let child_idx = get_cell_index(pos, scale_exp) ^ mirror_mask;

            if n_il || !bit_is_set_64(n_ml, n_mh, child_idx) { break; }

            let pi = popcount_below(n_ml, n_mh, child_idx);
            let child_node_idx = get_child_offset_pool(pool_base, node_idx, pi);
            let child_flags = get_node_flags_pool(pool_base, child_node_idx);

            if node_is_uniform_water(child_flags) {
                let water_frac = unmirror_pos(pos, dir);
                record_water_surface_hit(
                    ray_origin_world,
                    ray_dir_world,
                    (water_frac - vec3<f32>(1.0)) * ws,
                    last_axis,
                );
                break;
            }

            if camera.lod_bias > 0.0 {
                let child_world_size = ws * exp2(f32(i32(scale_exp) - i32(root_se)));
                let pos_frac = unmirror_pos(pos, dir);
                let pos_world = (pos_frac - vec3<f32>(1.0)) * ws;
                let lod_t = max(dot(pos_world - ray_origin_world, dir), 1.0);
                let projected = child_world_size / lod_t;
                let threshold = camera.pixel_size * camera.lod_bias * render_settings.node_lod_scale;

                if projected < threshold {
                    let avg_color = get_node_avg_color_pool(pool_base, child_node_idx);

                    result.hit = true;
                    result.is_lod_hit = true;
                    result.color_override = avg_color;
                    result.normal = axis_normal(last_axis, ray_dir_world);
                    result.lod_scale_exp = scale_exp;
                    result.hit_pos_local = pos_world;
                    result.t = dot(pos_world - ray_origin_world, dir);
                    return result;
                }
            }

            stk[scale_exp >> 1u] = StackEntry(node_idx, n_ml, n_mh, n_il);

            node_idx = child_node_idx;
            n_ml = pool_read(pool_base, node_idx);
            n_mh = pool_read(pool_base, node_idx + 1u);
            n_flags = child_flags;
            n_il = node_is_leaf(n_flags);
            scale_exp -= 2u;
            if ENABLE_TRACE_STATS { trace_stats_primary_descents += 1u; }
        }

        let child_idx = get_cell_index(pos, scale_exp) ^ mirror_mask;

        if n_il && bit_is_set_64(n_ml, n_mh, child_idx) {
            let mat = get_leaf_material_pool(pool_base, node_idx, child_idx);
            if mat != 0u {
                if (mat & WATER_BIT_MASK) != 0u {
                    // Water voxel — record surface hit and continue DDA
                    let dda_frac_w = unmirror_pos(pos, dir);
                    record_water_surface_hit(
                        ray_origin_world,
                        ray_dir_world,
                        (dda_frac_w - vec3<f32>(1.0)) * ws,
                        last_axis,
                    );
                    // Water disabled: treat as air — skip this voxel
                } else {
                    result.hit = true;
                    result.material = mat;
                    let dda_frac = unmirror_pos(pos, dir);
                    result.hit_pos_local = (dda_frac - vec3<f32>(1.0)) * ws;
                    result.t = dot(result.hit_pos_local - ray_origin_world, dir);
                    result.normal = axis_normal(last_axis, ray_dir_world);
                    return result;
                }
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
    // Reset DDA grass hit for this ray.
    dda_grass_hit.hit = false;
    dda_grass_hit.t = 1e20;

    // Reset water hit for this ray.
    dda_water_hit.hit = false;

    var result: HitResult;
    result.hit = false;
    result.is_lod_hit = false;
    result.lod_scale_exp = 0u;

    // No geometry below y=0; skip tracing entirely
    if ray_origin.y < 0.0 {
        return result;
    }

    let cs_xz = f32(streaming.chunk_size_xz);
    let cs_y = f32(streaming.chunk_size_y);
    let cs = vec3<f32>(cs_xz, cs_y, cs_xz);
    var dir = ray_dir;
    let eps_d = 1e-10;
    if abs(dir.x) < eps_d { dir.x = select(-eps_d, eps_d, dir.x >= 0.0); }
    if abs(dir.y) < eps_d { dir.y = select(-eps_d, eps_d, dir.y >= 0.0); }
    if abs(dir.z) < eps_d { dir.z = select(-eps_d, eps_d, dir.z >= 0.0); }

    let inv_dir = 1.0 / dir;

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
        return result;
    }

    var t_current = max(t_world.x, 0.0);

    let ray_eps = max(render_settings.ray_epsilon, 0.0);
    let first_entry = ray_origin + dir * (t_current + ray_eps);
    var cc = world_to_chunk_coord(
        clamp(first_entry, world_min + vec3<f32>(0.001), world_max - vec3<f32>(0.001)),
        cs_xz, cs_y,
    );

    let step = vec3<i32>(
        select(-1, 1, dir.x > 0.0),
        select(-1, 1, dir.y > 0.0),
        select(-1, 1, dir.z > 0.0),
    );
    let t_delta = abs(cs * inv_dir);

    var t_max = vec3<f32>(
        (f32(cc.x + select(0, 1, dir.x > 0.0)) * cs_xz - ray_origin.x) * inv_dir.x,
        (f32(cc.y + select(0, 1, dir.y > 0.0)) * cs_y - ray_origin.y) * inv_dir.y,
        (f32(cc.z + select(0, 1, dir.z > 0.0)) * cs_xz - ray_origin.z) * inv_dir.z,
    );

    var entry_axis: i32 = -1;

    let max_chunk_steps = u32(max(round(render_settings.max_chunk_steps), 1.0));
    for (var chunk_iter = 0u; chunk_iter < max_chunk_steps; chunk_iter++) {
        var do_grass = !skip_grass;

        // Grass hits are clamped to the current chunk segment in trace_grass_ray_bounded(),
        // so once the next chunk entry is past the best grass hit, no later chunk can
        // occlude it and the primary DDA can stop immediately.
        if do_grass && dda_grass_hit.hit && max(t_current, 0.0) >= dda_grass_hit.t {
            return result;
        }

        // Deep-water early-out: if the ray has traveled far enough underwater that
        // absorption fully converges to deep color, stop looking for seabed.
        // Skip when camera is underwater — the initial water hit is the surrounding
        // volume, not a surface the ray is looking through.
        if dda_water_hit.hit && camera.camera_underwater <= 0.5
            && (max(t_current, 0.0) - dda_water_hit.t) > WATER_DEEP_ABSORB_DIST {
            return result;
        }

        if ENABLE_TRACE_STATS {
            trace_stats_primary_chunk_steps += 1u;
            if dda_water_hit.hit {
                trace_stats_water_dda_chunks_behind += 1u;
            }
        }
        let info = lookup_chunk_info(cc);

        // Skip grass when the chunk's foliage band is deep enough below the water
        // surface that absorption makes it invisible. Uses vertical depth (Y),
        // not ray distance, so horizontal rays don't incorrectly skip surface-level grass.
        if do_grass && dda_water_hit.hit && camera.camera_underwater <= 0.5 {
            let water_surface_y = ray_origin.y + dir.y * dda_water_hit.t;
            let chunk_min_y = f32(cc.y) * cs_y;
            let foliage_top_world_y = chunk_min_y + f32(info.foliage_y_max);
            if (water_surface_y - foliage_top_world_y) > WATER_GRASS_SKIP_DEPTH {
                do_grass = false;
            }
        }

        if info.world_size != 0u {
            let pool_base = info.pool_offset;
            let root_flags = get_node_flags_pool(pool_base, info.root_offset);

            if camera.lod_bias > 0.0 && !node_is_uniform_water(root_flags) {
                let chunk_center = vec3<f32>(
                    (f32(cc.x) + 0.5) * cs_xz,
                    (f32(cc.y) + 0.5) * cs_y,
                    (f32(cc.z) + 0.5) * cs_xz,
                );
                let dist = max(length(chunk_center - ray_origin), 1.0);
                let projected = cs_xz / dist;
                if projected < camera.pixel_size * camera.lod_bias * render_settings.chunk_lod_scale {
                    let avg_color = get_node_avg_color_pool(pool_base, info.root_offset);
                    if avg_color.x > 0.001 || avg_color.y > 0.001 || avg_color.z > 0.001 {
                        result.hit = true;
                        result.is_lod_hit = true;
                        result.color_override = avg_color;
                        result.lod_scale_exp = 23u;
                        result.t = max(t_current, 0.0);
                        result.hit_pos_local = ray_origin + dir * result.t;
                        result.normal = axis_normal(entry_axis, ray_dir);
                        return result;
                    }
                }
            }

            let chunk_min = vec3<f32>(f32(cc.x) * cs_xz, f32(cc.y) * cs_y, f32(cc.z) * cs_xz);
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

            // Chunk exit t = next DDA boundary crossing (before stepping)
            let chunk_t_exit = min(t_max.x, min(t_max.y, t_max.z));

            if chunk_hit.hit {
                if do_grass {
                    // Before returning, check if grass in this chunk is closer.
                    if info.foliage_y_min < info.foliage_y_max {
                        let voxel_t = chunk_hit.t;
                        let grass_max = select(voxel_t, min(voxel_t, dda_grass_hit.t), dda_grass_hit.hit);
                        let foliage_base_y = chunk_min.y + f32(info.foliage_y_min);
                        let foliage_top_y = chunk_min.y + f32(info.foliage_y_max) + GRASS_BLADE_HEIGHT;
                        let grass = trace_grass_ray_bounded(
                            ray_origin, dir, camera.time, grass_max,
                            foliage_base_y, foliage_top_y,
                            max(t_current, 0.0), chunk_t_exit,
                            info.foliage_bitmap_offset, chunk_min.x, chunk_min.z, cs_xz,
                            chunk_min.y, info.foliage_y_bands,
                            info.foliage_tile_y_ranges_offset,
                        );
                        if grass.hit && grass.t < dda_grass_hit.t {
                            dda_grass_hit = grass;
                        }
                    }
                }

                // Grass hits are restricted to this chunk segment, so if the voxel is
                // farther away than the best grass hit we can stop immediately.
                if !do_grass || !dda_grass_hit.hit || chunk_hit.t <= dda_grass_hit.t {
                    var world_hit = chunk_hit;
                    world_hit.hit_pos_local = chunk_hit.hit_pos_local + chunk_min;
                    return world_hit;
                }
                return result;
            } else if do_grass && info.foliage_y_min < info.foliage_y_max {
                // No voxel hit in this chunk, but it has foliage — trace grass.
                let grass_max = select(1e20, dda_grass_hit.t, dda_grass_hit.hit);
                let foliage_base_y = chunk_min.y + f32(info.foliage_y_min);
                let foliage_top_y = chunk_min.y + f32(info.foliage_y_max) + GRASS_BLADE_HEIGHT;
                let grass = trace_grass_ray_bounded(
                    ray_origin, dir, camera.time, grass_max,
                    foliage_base_y, foliage_top_y,
                    max(t_current, 0.0), chunk_t_exit,
                    info.foliage_bitmap_offset, chunk_min.x, chunk_min.z, cs_xz,
                    chunk_min.y, info.foliage_y_bands,
                    info.foliage_tile_y_ranges_offset,
                );
                if grass.hit && grass.t < dda_grass_hit.t {
                    dda_grass_hit = grass;
                }
                if dda_grass_hit.hit {
                    return result;
                }
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

        // Early exit: ray went below world floor (y=0)
        if cc.y < 0 {
            break;
        }

        if t_current >= t_world.y {
            break;
        }
    }

    return result;
}

fn traverse_chunk_shadow(
    pool_base: u32,
    tree_info_ws: u32,
    tree_info_root: u32,
    tree_info_depth: u32,
    ray_origin_world: vec3<f32>,
    ray_dir_world: vec3<f32>,
    t_entry: f32,
) -> bool {
    let ws = f32(tree_info_ws);
    let depth = tree_info_depth;
    let root_se = 21u;
    let dir = ray_dir_world;

    let origin_frac = ray_origin_world / ws + vec3<f32>(1.0);
    let entry_world = ray_origin_world + dir * t_entry;
    var pos = clamp(entry_world / ws + vec3<f32>(1.0), vec3<f32>(1.0), vec3<f32>(1.9999999));
    let root_flags = get_node_flags_pool(pool_base, tree_info_root);
    if node_is_uniform_water(root_flags) {
        return false;
    }

    {
        var no = tree_info_root;
        var se = root_se;
        var ml = pool_read(pool_base, no);
        var mh = pool_read(pool_base, no + 1u);
        var flags = root_flags;
        var il = node_is_leaf(flags);

        for (var d = 0u; d < depth; d++) {
            let ci = get_cell_index(pos, se);
            if il {
                if bit_is_set_64(ml, mh, ci) {
                    let mat = get_leaf_material_pool(pool_base, no, ci);
                    if mat != 0u && (mat & WATER_BIT_MASK) == 0u {
                        return true;
                    }
                }
                break;
            }
            if !bit_is_set_64(ml, mh, ci) { break; }
            let pi = popcount_below(ml, mh, ci);
            let child_no = get_child_offset_pool(pool_base, no, pi);
            let child_flags = get_node_flags_pool(pool_base, child_no);
            if node_is_uniform_water(child_flags) {
                break;
            }
            no = child_no;
            ml = pool_read(pool_base, no);
            mh = pool_read(pool_base, no + 1u);
            flags = child_flags;
            il = node_is_leaf(flags);
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
    var n_flags = root_flags;
    var n_il = node_is_leaf(n_flags);
    var scale_exp = root_se;
    var side_dist = vec3<f32>(0.0);

    let max_steps = u32(max(round(render_settings.max_node_steps), 1.0));
    for (var i = 0u; i < max_steps; i++) {
        if ENABLE_TRACE_STATS { trace_stats_shadow_node_steps += 1u; }
        for (var dd = 0u; dd < depth; dd++) {
            let child_idx = get_cell_index(pos, scale_exp) ^ mirror_mask;
            if n_il || !bit_is_set_64(n_ml, n_mh, child_idx) { break; }

            let pi = popcount_below(n_ml, n_mh, child_idx);
            let child_node_idx = get_child_offset_pool(pool_base, node_idx, pi);
            let child_flags = get_node_flags_pool(pool_base, child_node_idx);
            if node_is_uniform_water(child_flags) { break; }

            stk[scale_exp >> 1u] = StackEntry(node_idx, n_ml, n_mh, n_il);
            node_idx = child_node_idx;
            n_ml = pool_read(pool_base, node_idx);
            n_mh = pool_read(pool_base, node_idx + 1u);
            n_flags = child_flags;
            n_il = node_is_leaf(n_flags);
            scale_exp -= 2u;
            if ENABLE_TRACE_STATS { trace_stats_shadow_descents += 1u; }
        }

        let child_idx = get_cell_index(pos, scale_exp) ^ mirror_mask;

        if n_il && bit_is_set_64(n_ml, n_mh, child_idx) {
            let mat = get_leaf_material_pool(pool_base, node_idx, child_idx);
            if mat != 0u && (mat & WATER_BIT_MASK) == 0u {
                return true;
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
            if diff_exp > i32(root_se) { break; }
            let se = stk[scale_exp >> 1u];
            node_idx = se.node_idx;
            n_ml = se.mask_lo;
            n_mh = se.mask_hi;
            n_il = se.is_leaf;
        }
    }

    return false;
}

fn trace_ao_ray(ray_origin: vec3<f32>, ray_dir: vec3<f32>, max_dist: f32) -> bool {
    let cs_xz = f32(streaming.chunk_size_xz);
    let cs_y = f32(streaming.chunk_size_y);
    let cs = vec3<f32>(cs_xz, cs_y, cs_xz);
    var dir = ray_dir;
    let eps_d = 1e-10;
    if abs(dir.x) < eps_d { dir.x = select(-eps_d, eps_d, dir.x >= 0.0); }
    if abs(dir.y) < eps_d { dir.y = select(-eps_d, eps_d, dir.y >= 0.0); }
    if abs(dir.z) < eps_d { dir.z = select(-eps_d, eps_d, dir.z >= 0.0); }

    let inv_dir = 1.0 / dir;

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

    var t_current = max(t_world.x, 0.0);

    let ray_eps = max(render_settings.ray_epsilon, 0.0);
    let first_entry = ray_origin + dir * (t_current + ray_eps);
    var cc = world_to_chunk_coord(
        clamp(first_entry, world_min + vec3<f32>(0.001), world_max - vec3<f32>(0.001)),
        cs_xz, cs_y,
    );

    let step = vec3<i32>(
        select(-1, 1, dir.x > 0.0),
        select(-1, 1, dir.y > 0.0),
        select(-1, 1, dir.z > 0.0),
    );
    let t_delta = abs(cs * inv_dir);

    var t_max = vec3<f32>(
        (f32(cc.x + select(0, 1, dir.x > 0.0)) * cs_xz - ray_origin.x) * inv_dir.x,
        (f32(cc.y + select(0, 1, dir.y > 0.0)) * cs_y - ray_origin.y) * inv_dir.y,
        (f32(cc.z + select(0, 1, dir.z > 0.0)) * cs_xz - ray_origin.z) * inv_dir.z,
    );

    var entry_axis: i32 = -1;

    let max_chunk_steps = u32(max(round(render_settings.max_chunk_steps), 1.0));
    for (var chunk_iter = 0u; chunk_iter < max_chunk_steps; chunk_iter++) {
        if ENABLE_TRACE_STATS { trace_stats_shadow_chunk_steps += 1u; }
        let info = lookup_chunk_info(cc);

        if info.world_size != 0u {
            let pool_base = info.pool_offset;
            let chunk_min = vec3<f32>(f32(cc.x) * cs_xz, f32(cc.y) * cs_y, f32(cc.z) * cs_xz);
            let local_origin = ray_origin - chunk_min;

            if traverse_chunk_shadow(
                pool_base,
                info.world_size,
                info.root_offset,
                info.depth,
                local_origin,
                dir,
                max(t_current, 0.0),
            ) {
                return true;
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

        if cc.y < 0 {
            break;
        }

        if t_current >= min(t_world.y, max_dist) {
            break;
        }
    }

    return false;
}

struct ReflectionHit {
    hit: bool,
    color: vec3<f32>,
    normal: vec3<f32>,
    world_pos: vec3<f32>,
};

// Trace a reflection ray through the voxel scene, returning the color of the
// first solid (non-water) voxel or grass hit within max_dist that is ABOVE water_y.
// Hits below the water surface are discarded so reflections only show geometry
// above the waterline, not the seabed visible through transparent water voxels.
fn trace_reflection_ray(
    ray_origin: vec3<f32>,
    ray_dir: vec3<f32>,
    max_dist: f32,
    water_y: f32,
) -> ReflectionHit {
    var refl: ReflectionHit;
    refl.hit = false;
    refl.color = vec3<f32>(0.0);
    refl.normal = vec3<f32>(0.0, 1.0, 0.0);
    refl.world_pos = vec3<f32>(0.0);

    let cs_xz = f32(streaming.chunk_size_xz);
    let cs_y = f32(streaming.chunk_size_y);
    let cs = vec3<f32>(cs_xz, cs_y, cs_xz);
    var dir = ray_dir;
    let eps_d = 1e-10;
    if abs(dir.x) < eps_d { dir.x = select(-eps_d, eps_d, dir.x >= 0.0); }
    if abs(dir.y) < eps_d { dir.y = select(-eps_d, eps_d, dir.y >= 0.0); }
    if abs(dir.z) < eps_d { dir.z = select(-eps_d, eps_d, dir.z >= 0.0); }

    let inv_dir = 1.0 / dir;

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
        return refl;
    }

    var t_current = max(t_world.x, 0.0);

    let ray_eps = max(render_settings.ray_epsilon, 0.0);
    let first_entry = ray_origin + dir * (t_current + ray_eps);
    var cc = world_to_chunk_coord(
        clamp(first_entry, world_min + vec3<f32>(0.001), world_max - vec3<f32>(0.001)),
        cs_xz, cs_y,
    );

    let step = vec3<i32>(
        select(-1, 1, dir.x > 0.0),
        select(-1, 1, dir.y > 0.0),
        select(-1, 1, dir.z > 0.0),
    );
    let t_delta = abs(cs * inv_dir);

    var t_max = vec3<f32>(
        (f32(cc.x + select(0, 1, dir.x > 0.0)) * cs_xz - ray_origin.x) * inv_dir.x,
        (f32(cc.y + select(0, 1, dir.y > 0.0)) * cs_y - ray_origin.y) * inv_dir.y,
        (f32(cc.z + select(0, 1, dir.z > 0.0)) * cs_xz - ray_origin.z) * inv_dir.z,
    );

    var entry_axis: i32 = -1;
    let do_grass = render_settings.vegetation_enabled > 0.5;
    var best_grass: GrassHit;
    best_grass.hit = false;
    best_grass.t = 1e20;

    let max_chunk_steps = u32(max(round(render_settings.max_chunk_steps), 1.0));
    for (var chunk_iter = 0u; chunk_iter < max_chunk_steps; chunk_iter++) {
        // If we already found grass and DDA has stepped past it, stop
        if do_grass && best_grass.hit && max(t_current, 0.0) >= best_grass.t {
            break;
        }

        let info = lookup_chunk_info(cc);

        if info.world_size != 0u {
            let pool_base = info.pool_offset;
            let chunk_min = vec3<f32>(f32(cc.x) * cs_xz, f32(cc.y) * cs_y, f32(cc.z) * cs_xz);
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

            let chunk_t_exit = min(t_max.x, min(t_max.y, t_max.z));

            if chunk_hit.hit {
                // Check for closer grass in this chunk before accepting voxel
                if do_grass && info.foliage_y_min < info.foliage_y_max {
                    let voxel_t = chunk_hit.t;
                    let grass_max = select(voxel_t, min(voxel_t, best_grass.t), best_grass.hit);
                    let foliage_base_y = chunk_min.y + f32(info.foliage_y_min);
                    let foliage_top_y = chunk_min.y + f32(info.foliage_y_max) + GRASS_BLADE_HEIGHT;
                    let grass = trace_grass_ray_bounded(
                        ray_origin, dir, camera.time, grass_max,
                        foliage_base_y, foliage_top_y,
                        max(t_current, 0.0), chunk_t_exit,
                        info.foliage_bitmap_offset, chunk_min.x, chunk_min.z, cs_xz,
                        chunk_min.y, info.foliage_y_bands,
                        info.foliage_tile_y_ranges_offset,
                    );
                    if grass.hit && grass.t < best_grass.t {
                        best_grass = grass;
                    }
                }

                // Grass closer than voxel — use grass
                if best_grass.hit && best_grass.t < chunk_hit.t {
                    if best_grass.pos.y >= water_y {
                        refl.hit = true;
                        refl.color = best_grass.color;
                        refl.normal = best_grass.normal;
                        refl.world_pos = best_grass.pos;
                    }
                    return refl;
                }

                // Voxel hit — check above water
                let hit_world_pos = chunk_hit.hit_pos_local + chunk_min;
                if hit_world_pos.y >= water_y {
                    refl.hit = true;
                    refl.normal = chunk_hit.normal;
                    refl.world_pos = hit_world_pos;
                    if chunk_hit.is_lod_hit {
                        refl.color = chunk_hit.color_override;
                    } else {
                        refl.color = render_settings.material_colors[min(chunk_hit.material & 0x3FFFu, 1023u)].rgb;
                    }
                    return refl;
                }
                // Hit is below water surface — stop
                return refl;
            } else if do_grass && info.foliage_y_min < info.foliage_y_max {
                // No voxel hit — trace grass in this chunk
                let grass_max = select(1e20, best_grass.t, best_grass.hit);
                let foliage_base_y = chunk_min.y + f32(info.foliage_y_min);
                let foliage_top_y = chunk_min.y + f32(info.foliage_y_max) + GRASS_BLADE_HEIGHT;
                let grass = trace_grass_ray_bounded(
                    ray_origin, dir, camera.time, grass_max,
                    foliage_base_y, foliage_top_y,
                    max(t_current, 0.0), chunk_t_exit,
                    info.foliage_bitmap_offset, chunk_min.x, chunk_min.z, cs_xz,
                    chunk_min.y, info.foliage_y_bands,
                    info.foliage_tile_y_ranges_offset,
                );
                if grass.hit && grass.t < best_grass.t {
                    best_grass = grass;
                }
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

        if cc.y < 0 {
            break;
        }

        if t_current >= min(t_world.y, max_dist) {
            break;
        }
    }

    // After loop: grass may have been found without a closer voxel
    if best_grass.hit && best_grass.pos.y >= water_y {
        refl.hit = true;
        refl.color = best_grass.color;
        refl.normal = best_grass.normal;
        refl.world_pos = best_grass.pos;
    }

    return refl;
}

fn trace_shadow_ray(ray_origin: vec3<f32>, ray_dir: vec3<f32>) -> bool {
    let cs_xz = f32(streaming.chunk_size_xz);
    let cs_y = f32(streaming.chunk_size_y);
    let cs = vec3<f32>(cs_xz, cs_y, cs_xz);
    var dir = ray_dir;
    let eps_d = 1e-10;
    if abs(dir.x) < eps_d { dir.x = select(-eps_d, eps_d, dir.x >= 0.0); }
    if abs(dir.y) < eps_d { dir.y = select(-eps_d, eps_d, dir.y >= 0.0); }
    if abs(dir.z) < eps_d { dir.z = select(-eps_d, eps_d, dir.z >= 0.0); }

    let inv_dir = 1.0 / dir;

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

    var t_current = max(t_world.x, 0.0);

    let ray_eps = max(render_settings.ray_epsilon, 0.0);
    let first_entry = ray_origin + dir * (t_current + ray_eps);
    var cc = world_to_chunk_coord(
        clamp(first_entry, world_min + vec3<f32>(0.001), world_max - vec3<f32>(0.001)),
        cs_xz, cs_y,
    );

    let step = vec3<i32>(
        select(-1, 1, dir.x > 0.0),
        select(-1, 1, dir.y > 0.0),
        select(-1, 1, dir.z > 0.0),
    );
    let t_delta = abs(cs * inv_dir);

    var t_max = vec3<f32>(
        (f32(cc.x + select(0, 1, dir.x > 0.0)) * cs_xz - ray_origin.x) * inv_dir.x,
        (f32(cc.y + select(0, 1, dir.y > 0.0)) * cs_y - ray_origin.y) * inv_dir.y,
        (f32(cc.z + select(0, 1, dir.z > 0.0)) * cs_xz - ray_origin.z) * inv_dir.z,
    );

    var entry_axis: i32 = -1;

    let max_chunk_steps = u32(max(round(render_settings.max_chunk_steps), 1.0));
    for (var chunk_iter = 0u; chunk_iter < max_chunk_steps; chunk_iter++) {
        if ENABLE_TRACE_STATS { trace_stats_shadow_chunk_steps += 1u; }
        let info = lookup_chunk_info(cc);

        if info.world_size != 0u {
            let pool_base = info.pool_offset;
            let chunk_min = vec3<f32>(f32(cc.x) * cs_xz, f32(cc.y) * cs_y, f32(cc.z) * cs_xz);
            let local_origin = ray_origin - chunk_min;

            if traverse_chunk_shadow(
                pool_base,
                info.world_size,
                info.root_offset,
                info.depth,
                local_origin,
                dir,
                max(t_current, 0.0),
            ) {
                return true;
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

        if cc.y < 0 {
            break;
        }

        if t_current >= t_world.y {
            break;
        }
    }

    return false;
}
