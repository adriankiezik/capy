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
