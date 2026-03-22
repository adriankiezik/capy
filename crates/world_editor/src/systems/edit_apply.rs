use std::collections::HashMap;
use std::sync::mpsc;
use std::time::Instant;

use bevy_ecs::system::{NonSendMut, Res, ResMut};
use capy_assets::VoxelPrefabAsset;
use capy_core::{MaterialId, MouseButton};
use capy_world::LeafBrickEdit;
use tracing::{error, info};

use crate::resources::{
    BrickChange, BrushShape, EditAction, EditHistory, EditTask, EditTaskOutput, EditableChunk,
    EditableWorld, EditorState, EditorTool, FoliageAction, InputEdge, MeshDirty, PendingEdits,
    PrefabLibrary, UpdatedChunk, VoxelHit, WorldGrid,
};

const BRICK: u32 = 4;

// ---------------------------------------------------------------------------
// Distance helpers (for sphere-brush voxel tools)
// ---------------------------------------------------------------------------

#[inline]
fn axis_min_dist_sq(target: i32, min: i32, max: i32) -> i32 {
    if target < min {
        let d = min - target;
        d * d
    } else if target > max {
        let d = target - max;
        d * d
    } else {
        0
    }
}

#[inline]
fn axis_max_dist_sq(target: i32, min: i32, max: i32) -> i32 {
    let d = (target - min).abs().max((target - max).abs());
    d * d
}

#[inline]
fn brick_distance_bounds_sq(target: [i32; 3], bx: u32, by: u32, bz: u32) -> (i32, i32) {
    let min = [
        (bx * BRICK) as i32,
        (by * BRICK) as i32,
        (bz * BRICK) as i32,
    ];
    let max = [
        min[0] + BRICK as i32 - 1,
        min[1] + BRICK as i32 - 1,
        min[2] + BRICK as i32 - 1,
    ];

    let min_dist_sq = axis_min_dist_sq(target[0], min[0], max[0])
        + axis_min_dist_sq(target[1], min[1], max[1])
        + axis_min_dist_sq(target[2], min[2], max[2]);
    let max_dist_sq = axis_max_dist_sq(target[0], min[0], max[0])
        + axis_max_dist_sq(target[1], min[1], max[1])
        + axis_max_dist_sq(target[2], min[2], max[2]);

    (min_dist_sq, max_dist_sq)
}

fn is_sculpt_tool(tool: EditorTool) -> bool {
    matches!(
        tool,
        EditorTool::Raise | EditorTool::Lower | EditorTool::Flatten | EditorTool::Smooth
    )
}

// ---------------------------------------------------------------------------
// Main edit_apply system
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
pub(crate) fn edit_apply(
    edge: Res<InputEdge>,
    voxel_hit: Option<Res<VoxelHit>>,
    state: Res<EditorState>,
    prefabs: Res<PrefabLibrary>,
    world_grid: Res<WorldGrid>,
    mut editable: ResMut<EditableWorld>,
    mut history: ResMut<EditHistory>,
    mut dirty: ResMut<MeshDirty>,
    mut pending: ResMut<PendingEdits>,
    mut task: NonSendMut<EditTask>,
) {
    if let Some(rx) = task.pending.take() {
        match rx.try_recv() {
            Ok(output) => {
                apply_edit_output(
                    &mut editable,
                    &mut history,
                    &mut dirty,
                    &mut pending,
                    output,
                );
            }
            Err(mpsc::TryRecvError::Empty) => {
                task.pending = Some(rx);
                return;
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                error!("[edit_apply] background worker disconnected before sending a result");
            }
        }
    }

    if !edge.mouse_just_pressed.contains(&MouseButton::Left) {
        return;
    }

    let Some(hit) = voxel_hit else {
        return;
    };
    if !hit.hit {
        return;
    }

    let tool = state.active_tool;
    if tool == EditorTool::Select {
        return;
    }
    if tool == EditorTool::Prefab {
        let Some(prefab) = prefabs.selected_prefab() else {
            return;
        };
        let rotation = state.prefab_rotation;
        let placement = hit.position + hit.normal * 0.5;
        let target = glam::IVec3::new(
            placement.x.floor() as i32,
            placement.y.floor() as i32,
            placement.z.floor() as i32,
        );
        let anchor = rotate_anchor_cw_y(
            glam::IVec3::from(prefab.anchor),
            prefab.size[0],
            prefab.size[2],
            rotation,
        );
        let origin = target - anchor;
        apply_prefab_placement(
            prefab,
            origin,
            rotation,
            world_grid.grid_dim_xz,
            &mut editable,
            &mut history,
            &mut dirty,
            &mut pending,
        );
        return;
    }

    let selected_material = state.selected_material;
    let brush_shape = state.brush_shape;
    let foliage_action = state.foliage_action;
    let r = state.brush_radius as i32;
    let cxz = capy_world::CHUNK_XZ as i32;
    let cy = capy_world::CHUNK_Y as i32;

    let target = match tool {
        EditorTool::Place => {
            let p = hit.position + hit.normal * 0.5;
            glam::IVec3::new(p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32)
        }
        _ => {
            let p = hit.position - hit.normal * 0.5;
            glam::IVec3::new(p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32)
        }
    };

    if is_sculpt_tool(tool) {
        let flatten_target_y = target.y as u32;
        let scan_r = if tool == EditorTool::Smooth { r + 1 } else { r };
        let cc_min = [
            (target.x - scan_r).div_euclid(cxz),
            0,
            (target.z - scan_r).div_euclid(cxz),
        ];
        let cc_max = [
            (target.x + scan_r).div_euclid(cxz),
            0,
            (target.z + scan_r).div_euclid(cxz),
        ];

        let chunk_snapshots = snapshot_affected_chunks(&editable, cc_min, cc_max);

        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let output = compute_sculpt_edit(
                target,
                tool,
                selected_material,
                r,
                brush_shape,
                flatten_target_y,
                chunk_snapshots,
            );
            let _ = tx.send(output);
        });
        task.pending = Some(rx);
    } else {
        let w_min = [target.x - r, target.y - r, target.z - r];
        let w_max = [target.x + r, target.y + r, target.z + r];
        let cc_min = [
            w_min[0].div_euclid(cxz),
            w_min[1].div_euclid(cy),
            w_min[2].div_euclid(cxz),
        ];
        let cc_max = [
            w_max[0].div_euclid(cxz),
            w_max[1].div_euclid(cy),
            w_max[2].div_euclid(cxz),
        ];

        let chunk_snapshots = snapshot_affected_chunks(&editable, cc_min, cc_max);

        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let output = compute_edit_output(
                target,
                tool,
                selected_material,
                r,
                brush_shape,
                foliage_action,
                chunk_snapshots,
            );
            let _ = tx.send(output);
        });
        task.pending = Some(rx);
    }
}

fn snapshot_affected_chunks(
    editable: &EditableWorld,
    cc_min: [i32; 3],
    cc_max: [i32; 3],
) -> HashMap<[i32; 3], EditableChunk> {
    let mut chunks = HashMap::new();

    for ccz in cc_min[2]..=cc_max[2] {
        for ccy in cc_min[1]..=cc_max[1] {
            for ccx in cc_min[0]..=cc_max[0] {
                let cc = [ccx, ccy, ccz];
                if let Some(chunk) = editable.chunks.get(&cc) {
                    chunks.insert(cc, chunk.clone());
                }
            }
        }
    }

    chunks
}

fn apply_prefab_placement(
    prefab: &VoxelPrefabAsset,
    origin: glam::IVec3,
    rotation: u8,
    grid_dim_xz: u32,
    editable: &mut EditableWorld,
    history: &mut EditHistory,
    dirty: &mut MeshDirty,
    pending: &mut PendingEdits,
) {
    let cxz = capy_world::CHUNK_XZ as i32;
    let half = (grid_dim_xz / 2) as i32;
    let min_x = -half * cxz;
    let max_x = (grid_dim_xz as i32 - half) * cxz - 1;
    let min_z = min_x;
    let max_z = max_x;
    let max_y = capy_world::CHUNK_Y as i32 - 1;

    // Compute rotated output dimensions.
    let (mut rsx, rsz) = (prefab.size[0], prefab.size[2]);
    let mut cur_sz = rsz;
    for _ in 0..rotation {
        let old = rsx;
        rsx = cur_sz;
        cur_sz = old;
    }

    let mut staged_bricks = HashMap::new();
    let mut placed_voxels = 0usize;
    let mut clipped_voxels = 0usize;

    for z in 0..prefab.size[2] {
        for y in 0..prefab.size[1] {
            for x in 0..prefab.size[0] {
                let material = prefab.voxel(x, y, z);
                if material == 0 {
                    continue;
                }

                // Apply rotation: (x,y,z) → rotated (dx, y, dz)
                let (mut dx, mut dz) = (x as i32, z as i32);
                let mut rot_sx = prefab.size[0] as i32;
                let mut rot_sz = prefab.size[2] as i32;
                for _ in 0..rotation {
                    let new_dx = dz;
                    let new_dz = rot_sx - 1 - dx;
                    dx = new_dx;
                    dz = new_dz;
                    let old = rot_sx;
                    rot_sx = rot_sz;
                    rot_sz = old;
                }

                let wx = origin.x + dx;
                let wy = origin.y + y as i32;
                let wz = origin.z + dz;

                if wx < min_x || wx > max_x || wy < 0 || wy > max_y || wz < min_z || wz > max_z {
                    clipped_voxels += 1;
                    continue;
                }

                placed_voxels += 1;

                let cc = [wx.div_euclid(cxz), 0, wz.div_euclid(cxz)];
                let lx = wx.rem_euclid(cxz) as u32;
                let ly = wy as u32;
                let lz = wz.rem_euclid(cxz) as u32;
                let brick = [lx / BRICK, ly / BRICK, lz / BRICK];
                let bit =
                    ((lx % BRICK) + (ly % BRICK) * BRICK + (lz % BRICK) * BRICK * BRICK) as usize;

                let entry = staged_bricks.entry((cc, brick)).or_insert_with(|| {
                    let old_brick = editable.chunks.get(&cc).map_or_else(
                        || EditableChunk::default().read_brick(brick[0], brick[1], brick[2]),
                        |chunk| chunk.read_brick(brick[0], brick[1], brick[2]),
                    );
                    (old_brick, old_brick)
                });
                entry.1[bit] = material;
            }
        }
    }

    if staged_bricks.is_empty() {
        return;
    }

    let mut changes = Vec::new();
    let mut pending_by_chunk: HashMap<[i32; 3], Vec<LeafBrickEdit>> = HashMap::new();

    for ((chunk_coord, brick_coord), (old_brick, new_brick)) in staged_bricks {
        if old_brick == new_brick {
            continue;
        }

        let chunk = editable.chunks.entry(chunk_coord).or_default();
        chunk.write_brick(brick_coord[0], brick_coord[1], brick_coord[2], new_brick);

        changes.push(BrickChange {
            chunk: chunk_coord,
            brick: brick_coord,
            old_materials: old_brick,
            new_materials: new_brick,
        });
        pending_by_chunk
            .entry(chunk_coord)
            .or_default()
            .push(LeafBrickEdit {
                bx: brick_coord[0],
                by: brick_coord[1],
                bz: brick_coord[2],
                materials: new_brick,
            });
    }

    if changes.is_empty() {
        return;
    }

    for (chunk_coord, edits) in pending_by_chunk {
        dirty.dirty.insert(chunk_coord);
        pending
            .by_chunk
            .entry(chunk_coord)
            .or_default()
            .extend(edits);
    }

    history.undo_stack.push(EditAction::Voxel { changes });
    history.redo_stack.clear();

    info!(
        "[prefab_place] prefab={}, voxels={}, clipped={clipped_voxels}, chunks={}",
        prefab.name,
        placed_voxels,
        dirty.dirty.len()
    );
}

/// Rotate an anchor point to match N 90° CW rotations around Y.
fn rotate_anchor_cw_y(mut anchor: glam::IVec3, mut sx: u32, sz: u32, n: u8) -> glam::IVec3 {
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

// ---------------------------------------------------------------------------
// Voxel tools (Place, Remove, Paint) — 3D sphere/cube brush
// ---------------------------------------------------------------------------

fn compute_edit_output(
    target: glam::IVec3,
    tool: EditorTool,
    selected_material: MaterialId,
    radius: i32,
    brush_shape: BrushShape,
    foliage_action: FoliageAction,
    mut chunks: HashMap<[i32; 3], EditableChunk>,
) -> EditTaskOutput {
    let cxz = capy_world::CHUNK_XZ as i32;
    let cy = capy_world::CHUNK_Y as i32;
    let r2 = radius * radius;
    let t_total = Instant::now();

    let w_min = [target.x - radius, target.y - radius, target.z - radius];
    let w_max = [target.x + radius, target.y + radius, target.z + radius];
    let cc_min = [
        w_min[0].div_euclid(cxz),
        w_min[1].div_euclid(cy),
        w_min[2].div_euclid(cxz),
    ];
    let cc_max = [
        w_max[0].div_euclid(cxz),
        w_max[1].div_euclid(cy),
        w_max[2].div_euclid(cxz),
    ];

    let brick_est = {
        let d = ((2 * radius + 3) / 4 + 1) as usize;
        d * d * d
    };
    let mut changes = Vec::with_capacity(brick_est);
    let mut changed_chunks = Vec::new();

    let t_loop = Instant::now();
    for ccz in cc_min[2]..=cc_max[2] {
        for ccy in cc_min[1]..=cc_max[1] {
            for ccx in cc_min[0]..=cc_max[0] {
                let cc = [ccx, ccy, ccz];
                let chunk = chunks.entry(cc).or_default();
                let org = [ccx * cxz, ccy * cy, ccz * cxz];

                let l_min = [
                    (w_min[0] - org[0]).max(0),
                    (w_min[1] - org[1]).max(0),
                    (w_min[2] - org[2]).max(0),
                ];
                let l_max = [
                    (w_max[0] - org[0]).min(cxz - 1),
                    (w_max[1] - org[1]).min(cy - 1),
                    (w_max[2] - org[2]).min(cxz - 1),
                ];

                let b_min = [
                    l_min[0] as u32 / BRICK,
                    l_min[1] as u32 / BRICK,
                    l_min[2] as u32 / BRICK,
                ];
                let b_max = [
                    l_max[0] as u32 / BRICK,
                    l_max[1] as u32 / BRICK,
                    l_max[2] as u32 / BRICK,
                ];

                let tl = [target.x - org[0], target.y - org[1], target.z - org[2]];

                let mut any_changed = false;
                let mut chunk_pending = Vec::new();

                for bz in b_min[2]..=b_max[2] {
                    for by in b_min[1]..=b_max[1] {
                        for bx in b_min[0]..=b_max[0] {
                            let old_brick = chunk.read_brick(bx, by, bz);
                            let mut new_brick = old_brick;

                            // Quick brick-level AABB test for sphere shape
                            // to skip bricks entirely outside the radius.
                            if brush_shape == BrushShape::Sphere {
                                let (min_dist_sq, _) = brick_distance_bounds_sq(tl, bx, by, bz);
                                if min_dist_sq > r2 {
                                    continue;
                                }
                            }

                            let mut changed = false;
                            let vx_base = (bx * BRICK) as i32;
                            let vy_base = (by * BRICK) as i32;
                            let vz_base = (bz * BRICK) as i32;

                            for lz in 0..BRICK as i32 {
                                let vz = vz_base + lz;
                                let dz = vz - tl[2];

                                for ly in 0..BRICK as i32 {
                                    let vy = vy_base + ly;
                                    let dy = vy - tl[1];

                                    for lx in 0..BRICK as i32 {
                                        let vx = vx_base + lx;
                                        let dx = vx - tl[0];

                                        if !voxel_in_brush(dx, dy, dz, r2, radius, brush_shape) {
                                            continue;
                                        }

                                        let bit = (lx
                                            + ly * BRICK as i32
                                            + lz * BRICK as i32 * BRICK as i32)
                                            as usize;
                                        let old = new_brick[bit];

                                        let new_mat = match tool {
                                            EditorTool::Place => {
                                                if old != 0 {
                                                    continue;
                                                }
                                                selected_material
                                            }
                                            EditorTool::Remove => {
                                                if old == 0 {
                                                    continue;
                                                }
                                                0
                                            }
                                            EditorTool::Paint => {
                                                if old == 0 {
                                                    continue;
                                                }
                                                selected_material
                                            }
                                            EditorTool::Foliage => {
                                                // Only affect the surface layer at the clicked Y level.
                                                if old == 0 {
                                                    continue;
                                                }
                                                // World-space Y of this voxel
                                                let wy = org[1] + vy_base + ly;
                                                if wy != target.y {
                                                    continue;
                                                }
                                                let above_is_air = if ly + 1 < BRICK as i32 {
                                                    // Within the same brick
                                                    let above_bit = (lx
                                                        + (ly + 1) * BRICK as i32
                                                        + lz * BRICK as i32 * BRICK as i32)
                                                        as usize;
                                                    new_brick[above_bit] == 0
                                                } else {
                                                    let above_wy = vy_base + ly + 1;
                                                    if above_wy >= cy {
                                                        true // top of chunk
                                                    } else {
                                                        let aby = by + 1;
                                                        let above_brick =
                                                            chunk.read_brick(bx, aby, bz);
                                                        let above_bit = (lx
                                                            + lz * BRICK as i32 * BRICK as i32)
                                                            as usize;
                                                        above_brick[above_bit] == 0
                                                    }
                                                };
                                                if !above_is_air {
                                                    continue; // not the surface voxel
                                                }
                                                match foliage_action {
                                                    FoliageAction::Paint => {
                                                        if capy_core::is_foliage_material(old) {
                                                            continue; // already has foliage
                                                        }
                                                        old | capy_core::FOLIAGE_BIT
                                                    }
                                                    FoliageAction::Erase => {
                                                        if !capy_core::is_foliage_material(old) {
                                                            continue; // no foliage to remove
                                                        }
                                                        old & !capy_core::FOLIAGE_BIT
                                                    }
                                                }
                                            }
                                            _ => continue,
                                        };

                                        if old == new_mat {
                                            continue;
                                        }

                                        new_brick[bit] = new_mat;
                                        changed = true;
                                    }
                                }
                            }

                            let brick_changed = changed;

                            if brick_changed {
                                chunk.write_brick(bx, by, bz, new_brick);

                                changes.push(BrickChange {
                                    chunk: cc,
                                    brick: [bx, by, bz],
                                    old_materials: old_brick,
                                    new_materials: new_brick,
                                });

                                chunk_pending.push(LeafBrickEdit {
                                    bx,
                                    by,
                                    bz,
                                    materials: new_brick,
                                });

                                any_changed = true;
                            }
                        }
                    }
                }

                if any_changed {
                    changed_chunks.push((cc, chunk_pending));
                }
            }
        }
    }
    let loop_ms = t_loop.elapsed().as_secs_f64() * 1000.0;

    let updated_chunks = changed_chunks
        .into_iter()
        .map(|(coord, pending)| UpdatedChunk {
            coord,
            chunk: chunks.remove(&coord).unwrap_or_default(),
            pending,
        })
        .collect();

    EditTaskOutput {
        updated_chunks,
        changes,
        loop_ms,
        worker_ms: t_total.elapsed().as_secs_f64() * 1000.0,
        radius,
    }
}

// ---------------------------------------------------------------------------
// Sculpt tools (Raise, Lower, Flatten, Smooth) — column-based height editing
// ---------------------------------------------------------------------------

/// Find the Y of the highest solid voxel in the column at local (lx, lz).
fn find_surface_height(chunk: &EditableChunk, lx: u32, lz: u32) -> Option<u32> {
    let bx = lx / BRICK;
    let bz = lz / BRICK;
    let local_x = lx % BRICK;
    let local_z = lz % BRICK;
    let brick_rows = capy_world::CHUNK_Y / BRICK;

    for by in (0..brick_rows).rev() {
        let brick = chunk.read_brick(bx, by, bz);
        for ly in (0..BRICK).rev() {
            let idx = (local_x + ly * BRICK + local_z * BRICK * BRICK) as usize;
            if brick[idx] != 0 {
                return Some(by * BRICK + ly);
            }
        }
    }
    None
}

/// Check if a world XZ column is within the 2D brush footprint.
#[inline]
fn column_in_brush(wx: i32, wz: i32, tx: i32, tz: i32, radius: i32, shape: BrushShape) -> bool {
    let dx = (wx - tx).abs();
    let dz = (wz - tz).abs();
    match shape {
        BrushShape::Sphere | BrushShape::Cylinder => dx * dx + dz * dz <= radius * radius,
        BrushShape::Cube => dx <= radius && dz <= radius,
        BrushShape::Diamond => dx + dz <= radius,
    }
}

/// Check if a voxel offset (dx, dy, dz) from the brush center is inside the 3D brush.
#[inline]
fn voxel_in_brush(dx: i32, dy: i32, dz: i32, r2: i32, radius: i32, shape: BrushShape) -> bool {
    match shape {
        BrushShape::Sphere => dx * dx + dy * dy + dz * dz <= r2,
        BrushShape::Cube => dx.abs() <= radius && dy.abs() <= radius && dz.abs() <= radius,
        BrushShape::Cylinder => dx * dx + dz * dz <= r2 && dy.abs() <= radius,
        BrushShape::Diamond => dx.abs() + dy.abs() + dz.abs() <= radius,
    }
}

/// Convert world XZ to chunk coord `[ccx, 0, ccz]` and local `(lx, lz)`.
#[inline]
fn world_to_chunk_local(wx: i32, wz: i32) -> ([i32; 3], u32, u32) {
    let cxz = capy_world::CHUNK_XZ as i32;
    let ccx = wx.div_euclid(cxz);
    let ccz = wz.div_euclid(cxz);
    let lx = wx.rem_euclid(cxz) as u32;
    let lz = wz.rem_euclid(cxz) as u32;
    ([ccx, 0, ccz], lx, lz)
}

/// Average surface height from a 3x3 neighborhood for smoothing.
fn neighbor_average(wx: i32, wz: i32, heights: &HashMap<(i32, i32), Option<u32>>) -> Option<u32> {
    let mut sum = 0u64;
    let mut count = 0u64;
    for dz in -1..=1i32 {
        for dx in -1..=1i32 {
            if let Some(&Some(h)) = heights.get(&(wx + dx, wz + dz)) {
                sum += h as u64;
                count += 1;
            }
        }
    }
    if count > 0 {
        Some((sum / count) as u32)
    } else {
        None
    }
}

/// Modify a column's height by filling or clearing voxels between current and target surface.
///
/// `original_bricks` tracks the pre-edit state of each brick so that multiple columns
/// sharing the same brick produce a single correct `BrickChange` (first old → final new).
#[allow(clippy::too_many_arguments)]
fn modify_column_height(
    chunk: &mut EditableChunk,
    lx: u32,
    lz: u32,
    current_h: Option<u32>,
    target_h: Option<u32>,
    material: MaterialId,
    cc: [i32; 3],
    original_bricks: &mut HashMap<([i32; 3], [u32; 3]), [MaterialId; 64]>,
) {
    if current_h == target_h {
        return;
    }

    // Express as "fill up to Y" (inclusive). -1 means entirely air.
    let cur_fill: i32 = current_h.map_or(-1, |h| h as i32);
    let tgt_fill: i32 = target_h.map_or(-1, |h| h as i32);

    if cur_fill == tgt_fill {
        return;
    }

    let bx = lx / BRICK;
    let bz = lz / BRICK;
    let local_x = lx % BRICK;
    let local_z = lz % BRICK;

    // Determine range and material to write (solid when raising, air when lowering).
    let (y_lo, y_hi, fill_mat) = if tgt_fill > cur_fill {
        ((cur_fill + 1) as u32, tgt_fill as u32, material)
    } else {
        ((tgt_fill + 1) as u32, cur_fill as u32, 0 as MaterialId)
    };

    let by_lo = y_lo / BRICK;
    let by_hi = y_hi / BRICK;

    for by in by_lo..=by_hi {
        let brick_key = (cc, [bx, by, bz]);
        // Snapshot the truly original brick the first time we touch it.
        original_bricks
            .entry(brick_key)
            .or_insert_with(|| chunk.read_brick(bx, by, bz));

        let mut current_brick = chunk.read_brick(bx, by, bz);
        let base_y = by * BRICK;
        let mut changed = false;

        for ly in 0..BRICK {
            let vy = base_y + ly;
            if vy >= y_lo && vy <= y_hi {
                let idx = (local_x + ly * BRICK + local_z * BRICK * BRICK) as usize;
                if current_brick[idx] != fill_mat {
                    current_brick[idx] = fill_mat;
                    changed = true;
                }
            }
        }

        if changed {
            chunk.write_brick(bx, by, bz, current_brick);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn compute_sculpt_edit(
    target: glam::IVec3,
    tool: EditorTool,
    selected_material: MaterialId,
    radius: i32,
    brush_shape: BrushShape,
    flatten_target_y: u32,
    mut chunks: HashMap<[i32; 3], EditableChunk>,
) -> EditTaskOutput {
    let t_total = Instant::now();

    // For smooth, scan a 1-voxel margin so edge columns have valid 3x3 neighbors.
    let scan_r = if tool == EditorTool::Smooth {
        radius + 1
    } else {
        radius
    };

    let mut all_columns = Vec::new();
    for wz in (target.z - scan_r)..=(target.z + scan_r) {
        for wx in (target.x - scan_r)..=(target.x + scan_r) {
            if column_in_brush(wx, wz, target.x, target.z, scan_r, brush_shape) {
                all_columns.push((wx, wz));
            }
        }
    }

    let t_loop = Instant::now();

    // Pre-scan surface heights for all columns (snapshot before modification).
    let heights: HashMap<(i32, i32), Option<u32>> = all_columns
        .iter()
        .map(|&(wx, wz)| {
            let (cc, lx, lz) = world_to_chunk_local(wx, wz);
            let chunk = chunks.entry(cc).or_default();
            ((wx, wz), find_surface_height(chunk, lx, lz))
        })
        .collect();

    // Track the truly original brick state (before any column in this edit touched it).
    let mut original_bricks: HashMap<([i32; 3], [u32; 3]), [MaterialId; 64]> = HashMap::new();

    for &(wx, wz) in &all_columns {
        // Only modify columns within the actual brush radius (skip margin).
        if !column_in_brush(wx, wz, target.x, target.z, radius, brush_shape) {
            continue;
        }

        let (cc, lx, lz) = world_to_chunk_local(wx, wz);
        let chunk = chunks.entry(cc).or_default();
        let current_h = heights.get(&(wx, wz)).copied().flatten();

        let new_h: Option<u32> = match tool {
            EditorTool::Raise => match current_h {
                Some(h) if h < capy_world::CHUNK_Y - 1 => Some(h + 1),
                Some(_) => current_h,
                None => Some(0),
            },
            EditorTool::Lower => current_h.and_then(|h| if h > 0 { Some(h - 1) } else { None }),
            EditorTool::Flatten => Some(flatten_target_y),
            EditorTool::Smooth => neighbor_average(wx, wz, &heights),
            _ => current_h,
        };

        modify_column_height(
            chunk,
            lx,
            lz,
            current_h,
            new_h,
            selected_material,
            cc,
            &mut original_bricks,
        );
    }

    let loop_ms = t_loop.elapsed().as_secs_f64() * 1000.0;

    // Build one BrickChange per unique brick: original old → final new.
    let mut changes = Vec::with_capacity(original_bricks.len());
    let mut changed_chunks: HashMap<[i32; 3], Vec<LeafBrickEdit>> = HashMap::new();

    for ((cc, brick), old_materials) in original_bricks {
        let chunk = chunks.entry(cc).or_default();
        let new_materials = chunk.read_brick(brick[0], brick[1], brick[2]);
        if old_materials == new_materials {
            continue;
        }
        changes.push(BrickChange {
            chunk: cc,
            brick,
            old_materials,
            new_materials,
        });
        changed_chunks.entry(cc).or_default().push(LeafBrickEdit {
            bx: brick[0],
            by: brick[1],
            bz: brick[2],
            materials: new_materials,
        });
    }

    let updated_chunks = changed_chunks
        .into_iter()
        .map(|(coord, pending)| UpdatedChunk {
            coord,
            chunk: chunks.remove(&coord).unwrap_or_default(),
            pending,
        })
        .collect();

    EditTaskOutput {
        updated_chunks,
        changes,
        loop_ms,
        worker_ms: t_total.elapsed().as_secs_f64() * 1000.0,
        radius,
    }
}

// ---------------------------------------------------------------------------
// Shared: apply result to ECS resources
// ---------------------------------------------------------------------------

fn apply_edit_output(
    editable: &mut EditableWorld,
    history: &mut EditHistory,
    dirty: &mut MeshDirty,
    pending: &mut PendingEdits,
    output: EditTaskOutput,
) {
    if output.changes.is_empty() {
        return;
    }

    let EditTaskOutput {
        updated_chunks,
        changes,
        loop_ms,
        worker_ms,
        radius,
    } = output;

    let t_apply = Instant::now();
    let num_changes = changes.len();
    let num_chunks = updated_chunks.len();

    for updated in updated_chunks {
        if updated.chunk.bricks.is_empty() {
            editable.chunks.remove(&updated.coord);
        } else {
            editable.chunks.insert(updated.coord, updated.chunk);
        }

        dirty.dirty.insert(updated.coord);
        pending
            .by_chunk
            .entry(updated.coord)
            .or_default()
            .extend(updated.pending);
    }

    history.undo_stack.push(EditAction::Voxel { changes });
    history.redo_stack.clear();

    let apply_ms = t_apply.elapsed().as_secs_f64() * 1000.0;
    info!(
        "[edit_apply] total={:.1}ms | worker={worker_ms:.1}ms, loop={loop_ms:.1}ms, \
         apply={apply_ms:.1}ms | bricks={num_changes}, chunks={num_chunks}, radius={radius}",
        worker_ms + apply_ms
    );
}
