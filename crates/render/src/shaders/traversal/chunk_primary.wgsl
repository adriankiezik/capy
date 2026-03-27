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
