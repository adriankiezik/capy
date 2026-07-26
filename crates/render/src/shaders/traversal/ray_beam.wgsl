// Beam pre-pass: one coarse ray per 8x8 pixel tile, returning a conservative
// lower bound on the hit distance for every pixel ray in that tile. Primary
// rays then start their chunk DDA at this t instead of at the camera, skipping
// the empty-space march that otherwise dominates traversal cost.
//
// Conservative rules:
//  - any occupied cell (solid OR water) stops the beam — no material reads
//  - descent stops once a child cell projects smaller than the beam footprint,
//    so the stop node is always at least as wide as the tile
//  - the returned t is pulled back by the stop-cell size plus a divergence
//    margin proportional to distance
//  - chunks with foliage cap the beam at the foliage slab entry, since grass
//    blades can be hit above the voxel surface

const BEAM_MISS_T: f32 = 1e30;
// The tile is 8 pixels wide; stop descending once a node projects below
// ~1-2 tiles so the beam stays cheap (the divergence margin keeps it safe).
// The scale folds in the 4x overestimate of the shared child_world_size
// formula (ws * exp2(scale_exp - root_se) is 4x the actual cell width).
const BEAM_TILE_PIXELS: f32 = 8.0;
const BEAM_THRESHOLD_SCALE: f32 = 4.0;
// Conservative bound on blade extent above the foliage surface, in voxels:
// grass.wgsl uses GRASS_BLADE_HEIGHT (5.0) plus a one-voxel root offset.
const BEAM_GRASS_BLADE_HEIGHT: f32 = 5.0;
const BEAM_GRASS_HEIGHT_PAD: f32 = 2.0;

var<private> beam_stk: array<u32, 6>;

fn beam_margin(t: f32, cell_ws: f32) -> f32 {
    return cell_ws + t * camera.pixel_size * BEAM_TILE_PIXELS;
}

// Traverse one chunk; returns a conservative stop t, or BEAM_MISS_T.
fn traverse_chunk_beam(
    pool_base: u32,
    tree_info_ws: u32,
    tree_info_root: u32,
    tree_info_depth: u32,
    ray_origin_world: vec3<f32>,
    ray_dir_world: vec3<f32>,
    t_entry: f32,
    threshold: f32,
) -> f32 {
    let ws = f32(tree_info_ws);
    let depth = tree_info_depth;
    let root_se = 21u;
    let dir = ray_dir_world;

    let origin_frac = ray_origin_world / ws + vec3<f32>(1.0);
    let entry_world = ray_origin_world + dir * t_entry;
    var pos = clamp(entry_world / ws + vec3<f32>(1.0), vec3<f32>(1.0), vec3<f32>(1.9999999));
    let root_flags = get_node_flags_pool(pool_base, tree_info_root);

    if node_is_uniform_water(root_flags) {
        return max(t_entry - beam_margin(t_entry, 0.0), 0.0);
    }

    // ── Entry phase: descend at the chunk entry point ──
    var node_idx = tree_info_root;
    var scale_exp = root_se;
    var n_ml = pool_read(pool_base, node_idx);
    var n_mh = pool_read(pool_base, node_idx + 1u);
    var n_il = node_is_leaf(root_flags);

    for (var d = 0u; d < depth; d++) {
        let ci = get_cell_index(pos, scale_exp);
        if n_il {
            if bit_is_set_64(n_ml, n_mh, ci) {
                return max(t_entry - beam_margin(t_entry, 0.0), 0.0);
            }
            break;
        }
        if !bit_is_set_64(n_ml, n_mh, ci) { break; }
        // Stop before descending below the beam footprint.
        let child_ws = ws * exp2(f32(i32(scale_exp) - i32(root_se)));
        if child_ws / max(t_entry, 1.0) < threshold {
            return max(t_entry - beam_margin(t_entry, child_ws), 0.0);
        }
        let pi = popcount_below(n_ml, n_mh, ci);
        let child_ptr = get_child_offset_pool(pool_base, node_idx, pi);
        if child_ptr_is_uniform_water(child_ptr) {
            return max(t_entry - beam_margin(t_entry, 0.0), 0.0);
        }
        beam_stk[(root_se - scale_exp) >> 1u] = node_idx;
        node_idx = child_ptr_offset(child_ptr);
        n_ml = pool_read(pool_base, node_idx);
        n_mh = pool_read(pool_base, node_idx + 1u);
        n_il = child_ptr_is_leaf(child_ptr);
        scale_exp -= 2u;
    }

    // ── DDA phase ──
    var mirror_mask = 0u;
    if dir.x > 0.0 { mirror_mask |= 3u; }
    if dir.y > 0.0 { mirror_mask |= 3u << 2u; }
    if dir.z > 0.0 { mirror_mask |= 3u << 4u; }

    let origin_m = mirror_pos(origin_frac, dir, true);
    pos = mirror_pos(pos, dir, false);
    let inv_dir = 1.0 / -abs(dir);

    var side_dist = vec3<f32>(0.0);

    let max_node_steps = u32(max(round(render_settings.max_node_steps), 1.0));
    for (var i = 0u; i < max_node_steps; i++) {
        for (var dd = 0u; dd < depth; dd++) {
            let child_idx = get_cell_index(pos, scale_exp) ^ mirror_mask;

            if n_il || !bit_is_set_64(n_ml, n_mh, child_idx) { break; }

            let pos_frac = unmirror_pos(pos, dir);
            let pos_world = (pos_frac - vec3<f32>(1.0)) * ws;
            let t = dot(pos_world - ray_origin_world, dir);
            let child_ws = ws * exp2(f32(i32(scale_exp) - i32(root_se)));
            if child_ws / max(t, 1.0) < threshold {
                return max(t - beam_margin(t, child_ws), 0.0);
            }

            let pi = popcount_below(n_ml, n_mh, child_idx);
            let child_ptr = get_child_offset_pool(pool_base, node_idx, pi);
            if child_ptr_is_uniform_water(child_ptr) {
                return max(t - beam_margin(t, child_ws), 0.0);
            }

            beam_stk[(root_se - scale_exp) >> 1u] = node_idx;
            node_idx = child_ptr_offset(child_ptr);
            n_ml = pool_read(pool_base, node_idx);
            n_mh = pool_read(pool_base, node_idx + 1u);
            n_il = child_ptr_is_leaf(child_ptr);
            scale_exp -= 2u;
        }

        let child_idx = get_cell_index(pos, scale_exp) ^ mirror_mask;

        if n_il && bit_is_set_64(n_ml, n_mh, child_idx) {
            let dda_frac = unmirror_pos(pos, dir);
            let hit_local = (dda_frac - vec3<f32>(1.0)) * ws;
            let t = dot(hit_local - ray_origin_world, dir);
            let cell_ws = ws * exp2(f32(i32(scale_exp) - i32(root_se)));
            return max(t - beam_margin(t, cell_ws), 0.0);
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
            if diff_exp > i32(root_se) {
                break;
            }
            node_idx = beam_stk[(root_se - scale_exp) >> 1u];
            n_ml = pool_read(pool_base, node_idx);
            n_mh = pool_read(pool_base, node_idx + 1u);
            n_il = false; // pushed nodes are never leaves
        }
    }

    return BEAM_MISS_T;
}

fn trace_beam(ray_origin: vec3<f32>, ray_dir: vec3<f32>) -> f32 {
    if ray_origin.y < 0.0 { return 0.0; }
    if !chunk_dda_init(ray_origin, ray_dir, 0.0) { return BEAM_MISS_T; }

    let threshold = camera.pixel_size * BEAM_TILE_PIXELS * BEAM_THRESHOLD_SCALE;
    let grass_blade = BEAM_GRASS_BLADE_HEIGHT
        * render_settings.vegetation_length * render_settings.vegetation_scale
        + BEAM_GRASS_HEIGHT_PAD;

    let max_steps = u32(max(round(render_settings.max_chunk_steps), 1.0));
    for (var i = 0u; i < max_steps; i++) {
        let info = lookup_chunk_info(dda.cc);

        if info.world_size != 0u {
            let chunk_min = chunk_dda_chunk_min();
            let t_enter = chunk_dda_t_enter();
            let t_exit = chunk_dda_t_exit();
            let local_origin = ray_origin - chunk_min;

            var result = BEAM_MISS_T;

            // Grass blades live above voxel surfaces inside the chunk's
            // foliage slab; the beam may not pass beyond the slab entry.
            if FEATURE_GRASS && info.foliage_y_min < info.foliage_y_max {
                let base_y = chunk_min.y + f32(info.foliage_y_min);
                let top_y = chunk_min.y + f32(info.foliage_y_max) + grass_blade;
                let ty0 = (base_y - ray_origin.y) / dda.dir.y;
                let ty1 = (top_y - ray_origin.y) / dda.dir.y;
                let slab_enter = max(min(ty0, ty1), t_enter);
                let slab_exit = min(max(ty0, ty1), t_exit);
                if slab_enter <= slab_exit {
                    result = max(slab_enter - beam_margin(slab_enter, 0.0), 0.0);
                }
            }

            // The solid AABB excludes water, so it may only clip the
            // traversal when water is rendered as air.
            var t_trace = t_enter;
            var skip_tree = false;
            if !FEATURE_WATER {
                if !slot_solid_aabb_valid(info) {
                    skip_tree = true;
                } else {
                    let solid_t = intersect_aabb(
                        local_origin,
                        dda.dir,
                        slot_solid_aabb_min(info),
                        slot_solid_aabb_max(info),
                    );
                    if solid_t.x >= solid_t.y || solid_t.y <= t_enter || solid_t.x >= t_exit {
                        skip_tree = true;
                    } else {
                        t_trace = max(t_enter, solid_t.x);
                    }
                }
            }

            if !skip_tree {
                let t = traverse_chunk_beam(
                    info.pool_offset, info.world_size, info.root_offset, info.depth,
                    local_origin, dda.dir, t_trace, threshold,
                );
                result = min(result, t);
            }

            if result < BEAM_MISS_T {
                return result;
            }
        }

        if !chunk_dda_step() { break; }
    }

    return BEAM_MISS_T;
}
