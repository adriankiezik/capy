struct HitSelection {
    use_water: bool,
    use_preview: bool,
    use_grass: bool,
    preview_hit: HitResult,
};

fn resolve_visible_hit(ray_origin: vec3<f32>, ray_dir: vec3<f32>, hit: HitResult) -> HitSelection {
    let grass = dda_grass_hit;
    var use_grass = FEATURE_GRASS && grass.hit && (!hit.hit || grass.t < hit.t);

    // Preview overlay: trace the prefab preview DAG if active
    var preview_hit_result: HitResult;
    preview_hit_result.hit = false;
    var use_preview = false;
    if preview.is_active != 0u {
        let pmin = vec3<f32>(preview.pos_x, preview.pos_y, preview.pos_z);
        let pmax = pmin + vec3<f32>(f32(preview.world_size));
        let t_pv = intersect_aabb(ray_origin, ray_dir, pmin, pmax);
        if t_pv.x < t_pv.y && t_pv.y > 0.0 {
            let local_o = ray_origin - pmin;
            let pv_hit = traverse_chunk(
                preview.pool_offset, preview.world_size,
                preview.root_offset, preview.depth,
                local_o, ray_dir, max(t_pv.x, 0.0), -1);
            if pv_hit.hit {
                let pv_pos = pv_hit.hit_pos_local + pmin;
                let pv_depth = pv_hit.t;
                // Compare with closest scene hit (grass or voxel)
                var scene_depth = 1e20;
                if use_grass {
                    scene_depth = grass.t;
                } else if hit.hit {
                    scene_depth = hit.t;
                }
                if pv_depth < scene_depth {
                    preview_hit_result = pv_hit;
                    preview_hit_result.hit_pos_local = pv_pos;
                    use_preview = true;
                    use_grass = false;
                }
            }
        }
    }

    // Check if a visible water surface is the closest hit.
    // When the camera starts underwater, the first recorded water hit belongs to
    // the surrounding water volume, so it should not suppress grass/voxel hits.
    let water = dda_water_hit;
    var use_water = FEATURE_WATER && water.hit && camera.camera_underwater <= 0.5;
    // Grass above water beats water
    if use_water && use_grass && grass.t < water.t {
        use_water = false;
    }
    // Preview in front of water beats water
    if use_water && use_preview && preview_hit_result.hit && preview_hit_result.t < water.t {
        use_water = false;
    }
    if use_water {
        // Water surface is in front — grass behind water is handled as underwater color
        use_grass = false;
    }

    return HitSelection(use_water, use_preview, use_grass, preview_hit_result);
}

fn visible_hit_depth(selection: HitSelection, hit: HitResult) -> f32 {
    if selection.use_water {
        return dda_water_hit.t;
    }
    if selection.use_preview {
        return selection.preview_hit.t;
    }
    if selection.use_grass {
        return dda_grass_hit.t;
    }
    return select(1e20, hit.t, hit.hit);
}
