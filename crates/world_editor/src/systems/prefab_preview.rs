use bevy_ecs::error::BevyError;
use bevy_ecs::world::World;
use capy_core::PreviewGpuData;
use tracing::info;

use crate::resources::{
    EditorState, EditorTool, PrefabLibrary, PreviewBake, SelectionPhase, SelectionState, VoxelHit,
};

/// When the selected prefab changes, bake it into a `BakedChunkData` and store
/// in `PreviewBake` so the rebake system can append it to the pool buffers.
pub(crate) fn prefab_preview_bake(world: &mut World) -> Result<(), BevyError> {
    let prefabs = world.resource::<PrefabLibrary>();
    let current_source = prefabs
        .selected_entry()
        .map(|entry| entry.source_path.clone());
    let current_generation = prefabs.prefab_generation;
    let rotation = world.resource::<EditorState>().prefab_rotation;

    let preview = world.resource::<PreviewBake>();
    if preview.source_path == current_source
        && preview.prefab_generation == current_generation
        && preview.rotation == rotation
        && (current_source.is_none() || preview.baked.is_some())
    {
        return Ok(());
    }

    let prefabs = world.resource::<PrefabLibrary>();
    let Some(prefab) = prefabs.selected_prefab() else {
        let mut preview = world.resource_mut::<PreviewBake>();
        preview.baked = None;
        preview.source_path = current_source;
        preview.prefab_generation = current_generation;
        preview.rotation = rotation;
        preview.needs_pool_rebuild = true;
        return Ok(());
    };

    let (mut voxels, mut sx, mut sy, mut sz) = (
        prefab.voxels.clone(),
        prefab.size[0],
        prefab.size[1],
        prefab.size[2],
    );

    // Apply 90° CW rotations around Y axis.
    for _ in 0..rotation {
        let (rotated, new_sx, new_sz) = rotate_voxels_cw_y(&voxels, sx, sy, sz);
        voxels = rotated;
        sx = new_sx;
        sz = new_sz;
    }

    // Pad the prefab voxels into a cubic power-of-4 grid.
    // bake_chunk's tree builder assumes grid dimensions match the power-of-4
    // world_size it computes internally; non-cubic grids cause out-of-bounds
    // slicing in the occupancy scan.
    let max_dim = sx.max(sy).max(sz);
    let padded = next_power_of_4(max_dim);
    let padded_data = pad_voxels(&voxels, sx, sy, sz, padded);
    let grid = capy_world::VoxelGrid::new(padded, padded, padded, padded_data)?;
    let baked = capy_world::bake_chunk(&grid, None)?;

    info!(
        "[prefab_preview] baked preview for '{}' ({}×{}×{}, rot={}, dag={})",
        prefab.name,
        sx,
        sy,
        sz,
        rotation,
        baked.dag_buffer.len()
    );

    let mut preview = world.resource_mut::<PreviewBake>();
    preview.baked = Some(baked);
    preview.source_path = current_source;
    preview.prefab_generation = current_generation;
    preview.rotation = rotation;
    preview.needs_pool_rebuild = true;

    Ok(())
}

/// Each frame when tool == Prefab and the cursor hits terrain, compute the
/// placement position and update `PreviewGpuData`. Otherwise deactivate.
pub(crate) fn prefab_preview_position(world: &mut World) -> Result<(), BevyError> {
    let state = world.resource::<EditorState>();
    let sel = world.resource::<SelectionState>();
    let is_pasting = sel.phase == SelectionPhase::Pasting;
    let is_prefab = state.active_tool == EditorTool::Prefab;

    if !is_prefab && !is_pasting {
        world.resource_mut::<PreviewGpuData>().active = false;
        return Ok(());
    }

    let hit = world.resource::<VoxelHit>();
    if !hit.hit {
        world.resource_mut::<PreviewGpuData>().active = false;
        return Ok(());
    }
    let hit_position = hit.position;
    let hit_normal = hit.normal;

    if is_pasting {
        // Clipboard paste preview — no anchor offset
        let preview_bake = world.resource::<PreviewBake>();
        let Some(ref baked) = preview_bake.baked else {
            world.resource_mut::<PreviewGpuData>().active = false;
            return Ok(());
        };
        let baked_world_size = baked.world_size;
        let baked_root_offset = baked.root_offset;
        let baked_depth = baked.depth;

        let placement = hit_position + hit_normal * 0.5;
        let origin = glam::IVec3::new(
            placement.x.floor() as i32,
            placement.y.floor() as i32,
            placement.z.floor() as i32,
        );

        let mut gpu = world.resource_mut::<PreviewGpuData>();
        gpu.active = true;
        gpu.position = [origin.x as f32, origin.y as f32, origin.z as f32];
        gpu.world_size = baked_world_size;
        gpu.root_offset = baked_root_offset;
        gpu.depth = baked_depth;
    } else {
        // Prefab preview
        let prefabs = world.resource::<PrefabLibrary>();
        let Some(prefab) = prefabs.selected_prefab() else {
            world.resource_mut::<PreviewGpuData>().active = false;
            return Ok(());
        };
        let rotation = world.resource::<EditorState>().prefab_rotation;
        let anchor = rotate_anchor(
            glam::IVec3::from(prefab.anchor),
            prefab.size[0],
            prefab.size[2],
            rotation,
        );

        let preview_bake = world.resource::<PreviewBake>();
        let Some(ref baked) = preview_bake.baked else {
            world.resource_mut::<PreviewGpuData>().active = false;
            return Ok(());
        };
        let baked_world_size = baked.world_size;
        let baked_root_offset = baked.root_offset;
        let baked_depth = baked.depth;

        let placement = hit_position + hit_normal * 0.5;
        let target = glam::IVec3::new(
            placement.x.floor() as i32,
            placement.y.floor() as i32,
            placement.z.floor() as i32,
        );
        let origin = target - anchor;

        let mut gpu = world.resource_mut::<PreviewGpuData>();
        gpu.active = true;
        gpu.position = [origin.x as f32, origin.y as f32, origin.z as f32];
        gpu.world_size = baked_world_size;
        gpu.root_offset = baked_root_offset;
        gpu.depth = baked_depth;
    }
    // pool_offset is set by the rebake system when it appends the DAG

    Ok(())
}

fn next_power_of_4(size: u32) -> u32 {
    if size <= 1 {
        return 1;
    }
    let mut p = 1u32;
    while p < size {
        p *= 4;
    }
    p
}

/// Rotate a flat voxel buffer 90° CW around Y: `(x,y,z) → (z, y, sx-1-x)`.
/// Returns the rotated buffer and the new (sx, sz) — sy is unchanged.
fn rotate_voxels_cw_y(
    src: &[capy_core::MaterialId],
    sx: u32,
    sy: u32,
    sz: u32,
) -> (Vec<capy_core::MaterialId>, u32, u32) {
    // New dimensions: [sz, sy, sx]
    let (nsx, nsz) = (sz, sx);
    let total = (nsx as usize) * (sy as usize) * (nsz as usize);
    let mut dst = vec![0 as capy_core::MaterialId; total];
    for z in 0..sz {
        for y in 0..sy {
            for x in 0..sx {
                let src_idx = (x + y * sx + z * sx * sy) as usize;
                let dx = z;
                let dy = y;
                let dz = sx - 1 - x;
                let dst_idx = (dx + dy * nsx + dz * nsx * sy) as usize;
                dst[dst_idx] = src[src_idx];
            }
        }
    }
    (dst, nsx, nsz)
}

/// Rotate an anchor point to match `rotate_voxels_cw_y` applied `n` times.
fn rotate_anchor(mut anchor: glam::IVec3, mut sx: u32, sz: u32, n: u8) -> glam::IVec3 {
    let mut cur_sz = sz;
    for _ in 0..n {
        let new_x = anchor.z;
        let new_z = sx as i32 - 1 - anchor.x;
        anchor.x = new_x;
        anchor.z = new_z;
        let old_sx = sx;
        sx = cur_sz;
        cur_sz = old_sx;
    }
    anchor
}

/// Copy prefab voxels (sx × sy × sz, x-major) into a zero-padded cubic grid.
fn pad_voxels(
    src: &[capy_core::MaterialId],
    sx: u32,
    sy: u32,
    sz: u32,
    padded: u32,
) -> Vec<capy_core::MaterialId> {
    let total = (padded as usize) * (padded as usize) * (padded as usize);
    let mut dst = vec![0 as capy_core::MaterialId; total];
    let p = padded as usize;
    for z in 0..sz as usize {
        for y in 0..sy as usize {
            let src_row = z * (sx as usize) * (sy as usize) + y * (sx as usize);
            let dst_row = z * p * p + y * p;
            dst[dst_row..dst_row + sx as usize]
                .copy_from_slice(&src[src_row..src_row + sx as usize]);
        }
    }
    dst
}
