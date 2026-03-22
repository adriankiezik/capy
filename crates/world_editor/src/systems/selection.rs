use std::collections::HashMap;

use bevy_ecs::system::{Res, ResMut};
use capy_core::{Camera, GameWindow, KeyCode, MaterialId, MouseButton, RawInput};
use capy_ui::EguiContext;
use capy_world::LeafBrickEdit;
use glam::{IVec3, Vec3};
use tracing::info;

use crate::resources::{
    BrickChange, Clipboard, EditAction, EditHistory, EditableChunk, EditableWorld, EditorState,
    EditorTool, Face, InputEdge, MeshDirty, PendingEdits, SelectionPhase, SelectionState, VoxelHit,
    WorldGrid,
};

const BRICK: u32 = 4;
type StagedBricks = HashMap<([i32; 3], [u32; 3]), ([MaterialId; 64], [MaterialId; 64])>;

// ---------------------------------------------------------------------------
// Ray helpers
// ---------------------------------------------------------------------------

/// Compute the mouse ray direction from camera + cursor screen position.
fn mouse_ray_dir(camera: &Camera, cursor: egui::Pos2, screen_w: f32, screen_h: f32) -> Vec3 {
    let ndc_x = (cursor.x / screen_w) * 2.0 - 1.0;
    let ndc_y = 1.0 - (cursor.y / screen_h) * 2.0;

    let forward = camera.forward();
    let right = camera.right();
    let up = right.cross(forward).normalize();

    let half_h = (camera.fov_y * 0.5).tan();
    let half_w = half_h * camera.aspect;

    (forward + right * (ndc_x * half_w) + up * (ndc_y * half_h)).normalize()
}

/// Ray vs axis-aligned box. Returns (t_enter, entry Face).
/// `aabb_min`/`aabb_max` are in world-space floats (voxel-expanded).
fn ray_aabb(origin: Vec3, dir: Vec3, aabb_min: Vec3, aabb_max: Vec3) -> Option<(f32, Face)> {
    let inv = Vec3::new(
        if dir.x.abs() > 1e-9 {
            1.0 / dir.x
        } else {
            f32::copysign(1e9, dir.x)
        },
        if dir.y.abs() > 1e-9 {
            1.0 / dir.y
        } else {
            f32::copysign(1e9, dir.y)
        },
        if dir.z.abs() > 1e-9 {
            1.0 / dir.z
        } else {
            f32::copysign(1e9, dir.z)
        },
    );

    let t1 = (aabb_min - origin) * inv;
    let t2 = (aabb_max - origin) * inv;

    let t_min = t1.min(t2);
    let t_max = t1.max(t2);

    let t_enter = t_min.x.max(t_min.y).max(t_min.z);
    let t_exit = t_max.x.min(t_max.y).min(t_max.z);

    if t_enter > t_exit || t_exit < 0.0 {
        return None;
    }

    let face = if t_min.x >= t_min.y && t_min.x >= t_min.z {
        if dir.x > 0.0 { Face::XNeg } else { Face::XPos }
    } else if t_min.y >= t_min.x && t_min.y >= t_min.z {
        if dir.y > 0.0 { Face::YNeg } else { Face::YPos }
    } else if dir.z > 0.0 {
        Face::ZNeg
    } else {
        Face::ZPos
    };

    Some((t_enter.max(0.0), face))
}

/// Given a mouse ray and a drag axis (0=X, 1=Y, 2=Z), compute the world-space
/// coordinate along that axis by intersecting with the best helper plane.
fn drag_along_axis(origin: Vec3, dir: Vec3, axis: usize, box_center: Vec3) -> Option<f32> {
    // Two candidate planes perpendicular to the non-drag axes, through box center
    let candidates: [(Vec3, f32); 2] = match axis {
        0 => [(Vec3::Y, box_center.y), (Vec3::Z, box_center.z)],
        1 => [(Vec3::X, box_center.x), (Vec3::Z, box_center.z)],
        _ => [(Vec3::X, box_center.x), (Vec3::Y, box_center.y)],
    };

    // Pick the plane more perpendicular to the ray for numerical stability
    let (normal, d) = if dir.dot(candidates[0].0).abs() > dir.dot(candidates[1].0).abs() {
        candidates[0]
    } else {
        candidates[1]
    };

    let denom = dir.dot(normal);
    if denom.abs() < 1e-6 {
        return None;
    }
    let t = (d - origin.dot(normal)) / denom;
    if t < 0.0 {
        return None;
    }
    let hit = origin + dir * t;
    Some(match axis {
        0 => hit.x,
        1 => hit.y,
        _ => hit.z,
    })
}

/// Intersect ray with a horizontal (XZ) plane at the given Y height.
/// Returns the world-space hit point.
fn ray_y_plane(origin: Vec3, dir: Vec3, y: f32) -> Option<Vec3> {
    if dir.y.abs() < 1e-6 {
        return None;
    }
    let t = (y - origin.y) / dir.y;
    if t < 0.0 {
        return None;
    }
    Some(origin + dir * t)
}

fn hit_to_voxel(hit: &VoxelHit) -> IVec3 {
    let p = hit.position - hit.normal * 0.5;
    IVec3::new(p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32)
}

fn hit_to_place_voxel(hit: &VoxelHit) -> IVec3 {
    let p = hit.position + hit.normal * 0.5;
    IVec3::new(p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32)
}

/// Build the AABB in float space from integer selection corners.
/// Expands by +1 on max side so the box wraps around voxel cubes.
fn selection_aabb_f32(min: IVec3, max: IVec3) -> (Vec3, Vec3) {
    (
        Vec3::new(min.x as f32, min.y as f32, min.z as f32),
        Vec3::new(max.x as f32 + 1.0, max.y as f32 + 1.0, max.z as f32 + 1.0),
    )
}

// ---------------------------------------------------------------------------
// Main system
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
pub(crate) fn selection_system(
    egui_ctx: Res<EguiContext>,
    input: Res<RawInput>,
    edge: Res<InputEdge>,
    voxel_hit: Option<Res<VoxelHit>>,
    state: Res<EditorState>,
    camera: Res<Camera>,
    window: Res<GameWindow>,
    world_grid: Res<WorldGrid>,
    mut sel: ResMut<SelectionState>,
    mut clipboard: ResMut<Clipboard>,
    mut editable: ResMut<EditableWorld>,
    mut history: ResMut<EditHistory>,
    mut dirty: ResMut<MeshDirty>,
    mut pending: ResMut<PendingEdits>,
) {
    if state.active_tool != EditorTool::Select {
        return;
    }

    let ctrl_held = input.keys_held.contains(&KeyCode::ControlLeft)
        || input.keys_held.contains(&KeyCode::ControlRight);

    // Keyboard shortcuts (work regardless of egui focus)
    match sel.phase {
        SelectionPhase::Selected => {
            if edge.keys_just_pressed.contains(&KeyCode::Escape) {
                sel.clear();
                return;
            }
            if ctrl_held && edge.keys_just_pressed.contains(&KeyCode::KeyC) {
                copy_selection(&editable, &sel, &mut clipboard);
                return;
            }
            if ctrl_held && edge.keys_just_pressed.contains(&KeyCode::KeyX) {
                copy_selection(&editable, &sel, &mut clipboard);
                clear_selection(&sel, &mut editable, &mut history, &mut dirty, &mut pending);
                sel.clear();
                return;
            }
            if edge.keys_just_pressed.contains(&KeyCode::Delete) {
                clear_selection(&sel, &mut editable, &mut history, &mut dirty, &mut pending);
                sel.clear();
                return;
            }
            if ctrl_held && edge.keys_just_pressed.contains(&KeyCode::KeyV) && !clipboard.is_empty()
            {
                sel.phase = SelectionPhase::Pasting;
                return;
            }
            // G = grab (move)
            if edge.keys_just_pressed.contains(&KeyCode::KeyG) {
                if let (Some(min), Some(max)) = (sel.aabb_min(), sel.aabb_max()) {
                    sel.pre_move_a = sel.corner_a;
                    sel.pre_move_b = sel.corner_b;
                    let center_y = (min.y + max.y) as f32 / 2.0;
                    // Compute initial grab point
                    let cursor = egui_ctx.0.input(|i| i.pointer.latest_pos());
                    let sw = window.width as f32;
                    let sh = window.height as f32;
                    if let Some(pos) = cursor {
                        let dir = mouse_ray_dir(&camera, pos, sw, sh);
                        if let Some(hit) = ray_y_plane(camera.position, dir, center_y) {
                            sel.grab_point = hit;
                        }
                    }
                    sel.phase = SelectionPhase::Moving;
                }
                return;
            }
            // R = rotate 90° CW around Y axis
            if edge.keys_just_pressed.contains(&KeyCode::KeyR) {
                rotate_selection_in_place(
                    &mut sel,
                    &mut editable,
                    &mut history,
                    &mut dirty,
                    &mut pending,
                );
                return;
            }
        }
        SelectionPhase::Moving => {
            if edge.keys_just_pressed.contains(&KeyCode::Escape) {
                // Cancel move — restore original corners
                sel.corner_a = sel.pre_move_a;
                sel.corner_b = sel.pre_move_b;
                sel.pre_move_a = None;
                sel.pre_move_b = None;
                sel.phase = SelectionPhase::Selected;
                return;
            }
        }
        SelectionPhase::Pasting => {
            if edge.keys_just_pressed.contains(&KeyCode::Escape) {
                if sel.corner_a.is_some() && sel.corner_b.is_some() {
                    sel.phase = SelectionPhase::Selected;
                } else {
                    sel.clear();
                }
                return;
            }
            if edge.keys_just_pressed.contains(&KeyCode::KeyR) {
                clipboard.rotate_cw_y();
                info!(
                    "[selection] rotated clipboard 90° CW, new size={}x{}x{}",
                    clipboard.size[0], clipboard.size[1], clipboard.size[2],
                );
                return;
            }
        }
        _ => {
            if edge.keys_just_pressed.contains(&KeyCode::Escape) {
                sel.clear();
                return;
            }
        }
    }

    let sw = window.width as f32;
    let sh = window.height as f32;
    let cursor = egui_ctx.0.input(|i| i.pointer.latest_pos());

    match sel.phase {
        SelectionPhase::None => {
            if edge.mouse_just_pressed.contains(&MouseButton::Left)
                && let Some(hit) = voxel_hit.as_ref().filter(|h| h.hit)
            {
                let voxel = hit_to_voxel(hit);
                sel.corner_a = Some(voxel);
                sel.corner_b = Some(voxel);
                sel.phase = SelectionPhase::Dragging;
            }
        }
        SelectionPhase::Dragging => {
            if let Some(hit) = voxel_hit.as_ref().filter(|h| h.hit) {
                sel.corner_b = Some(hit_to_voxel(hit));
            }
            if edge.mouse_just_released.contains(&MouseButton::Left) {
                sel.phase = SelectionPhase::Selected;
            }
        }
        SelectionPhase::Selected => {
            if edge.mouse_just_pressed.contains(&MouseButton::Left)
                && let (Some(pos), Some(min), Some(max)) = (cursor, sel.aabb_min(), sel.aabb_max())
            {
                let dir = mouse_ray_dir(&camera, pos, sw, sh);
                let (fmin, fmax) = selection_aabb_f32(min, max);

                if let Some((_t, face)) = ray_aabb(camera.position, dir, fmin, fmax) {
                    // Ray hit the selection box — resize that face
                    let axis = face.axis();
                    let center = Vec3::new(
                        (min.x + max.x) as f32 / 2.0,
                        (min.y + max.y) as f32 / 2.0,
                        (min.z + max.z) as f32 / 2.0,
                    );
                    // Store the original corner value and initial drag position
                    // so we can compute deltas and avoid jumps on click.
                    let origin_val = match face {
                        Face::XNeg | Face::YNeg | Face::ZNeg => [min.x, min.y, min.z][axis],
                        _ => [max.x, max.y, max.z][axis],
                    };
                    sel.resize_origin = origin_val;
                    sel.resize_anchor = drag_along_axis(camera.position, dir, axis, center)
                        .unwrap_or(origin_val as f32);
                    sel.resize_face = Some(face);
                    sel.phase = SelectionPhase::Resizing;
                } else if let Some(hit) = voxel_hit.as_ref().filter(|h| h.hit) {
                    // Ray missed the box — start a new selection
                    let voxel = hit_to_voxel(hit);
                    sel.corner_a = Some(voxel);
                    sel.corner_b = Some(voxel);
                    sel.phase = SelectionPhase::Dragging;
                }
            }
        }
        SelectionPhase::Resizing => {
            if let (Some(pos), Some(face), Some(min), Some(max)) =
                (cursor, sel.resize_face, sel.aabb_min(), sel.aabb_max())
            {
                let dir = mouse_ray_dir(&camera, pos, sw, sh);
                let center = Vec3::new(
                    (min.x + max.x) as f32 / 2.0,
                    (min.y + max.y) as f32 / 2.0,
                    (min.z + max.z) as f32 / 2.0,
                );
                let axis = face.axis();

                if let Some(val) = drag_along_axis(camera.position, dir, axis, center) {
                    // Delta-based: compute how far the mouse moved from the
                    // initial click and apply that to the original corner value.
                    let delta = val - sel.resize_anchor;
                    let v = sel.resize_origin + delta.round() as i32;
                    let (new_min, new_max) = match face {
                        Face::XNeg => (IVec3::new(v, min.y, min.z), max),
                        Face::XPos => (min, IVec3::new(v, max.y, max.z)),
                        Face::YNeg => (IVec3::new(min.x, v, min.z), max),
                        Face::YPos => (min, IVec3::new(max.x, v, max.z)),
                        Face::ZNeg => (IVec3::new(min.x, min.y, v), max),
                        Face::ZPos => (min, IVec3::new(max.x, max.y, v)),
                    };
                    sel.corner_a = Some(new_min);
                    sel.corner_b = Some(new_max);
                }
            }

            if edge.mouse_just_released.contains(&MouseButton::Left) {
                if let (Some(a), Some(b)) = (sel.corner_a, sel.corner_b) {
                    sel.corner_a = Some(a.min(b));
                    sel.corner_b = Some(a.max(b));
                }
                sel.resize_face = None;
                sel.phase = SelectionPhase::Selected;
            }
        }
        SelectionPhase::Moving => {
            // Track cursor on the horizontal plane at the original grab height
            if let (Some(pos), Some(pre_a), Some(pre_b)) = (cursor, sel.pre_move_a, sel.pre_move_b)
            {
                let dir = mouse_ray_dir(&camera, pos, sw, sh);
                if let Some(hit) = ray_y_plane(camera.position, dir, sel.grab_point.y) {
                    let delta = hit - sel.grab_point;
                    let dx = delta.x.round() as i32;
                    let dz = delta.z.round() as i32;
                    let offset = IVec3::new(dx, 0, dz);
                    sel.corner_a = Some(pre_a + offset);
                    sel.corner_b = Some(pre_b + offset);
                }
            }

            if edge.mouse_just_pressed.contains(&MouseButton::Left) {
                // Confirm the move
                sel.pre_move_a = None;
                sel.pre_move_b = None;
                sel.phase = SelectionPhase::Selected;
            }
        }
        SelectionPhase::Pasting => {
            if edge.mouse_just_pressed.contains(&MouseButton::Left)
                && let Some(hit) = voxel_hit.as_ref().filter(|h| h.hit)
            {
                let origin = hit_to_place_voxel(hit);
                paste_clipboard(
                    &clipboard,
                    origin,
                    sel.include_air,
                    world_grid.grid_dim_xz,
                    &mut editable,
                    &mut history,
                    &mut dirty,
                    &mut pending,
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Clipboard operations
// ---------------------------------------------------------------------------

fn copy_selection(editable: &EditableWorld, sel: &SelectionState, clipboard: &mut Clipboard) {
    let Some(min) = sel.aabb_min() else { return };
    let Some(max) = sel.aabb_max() else { return };

    let dims = max - min + IVec3::ONE;
    clipboard.bricks.clear();
    clipboard.size = [dims.x as u32, dims.y as u32, dims.z as u32];

    let cxz = capy_world::CHUNK_XZ as i32;
    let cy = capy_world::CHUNK_Y as i32;

    let b = BRICK as i32;
    let wb_min = [
        min.x.div_euclid(b),
        min.y.div_euclid(b),
        min.z.div_euclid(b),
    ];
    let wb_max = [
        max.x.div_euclid(b),
        max.y.div_euclid(b),
        max.z.div_euclid(b),
    ];

    let default_chunk = EditableChunk::default();

    for wbz in wb_min[2]..=wb_max[2] {
        for wby in wb_min[1]..=wb_max[1] {
            for wbx in wb_min[0]..=wb_max[0] {
                let brick_origin = IVec3::new(wbx * b, wby * b, wbz * b);

                let cc = [
                    brick_origin.x.div_euclid(cxz),
                    brick_origin.y.div_euclid(cy),
                    brick_origin.z.div_euclid(cxz),
                ];
                let lx = brick_origin.x.rem_euclid(cxz) as u32 / BRICK;
                let ly = brick_origin.y.rem_euclid(cy) as u32 / BRICK;
                let lz = brick_origin.z.rem_euclid(cxz) as u32 / BRICK;

                let chunk = editable.chunks.get(&cc).unwrap_or(&default_chunk);
                let src_brick = chunk.read_brick(lx, ly, lz);

                let rel_bx = (wbx - wb_min[0]) as u32;
                let rel_by = (wby - wb_min[1]) as u32;
                let rel_bz = (wbz - wb_min[2]) as u32;

                let brick_max = brick_origin + IVec3::splat(b - 1);
                let fully_inside = brick_origin.x >= min.x
                    && brick_origin.y >= min.y
                    && brick_origin.z >= min.z
                    && brick_max.x <= max.x
                    && brick_max.y <= max.y
                    && brick_max.z <= max.z;

                if fully_inside {
                    if sel.include_air || src_brick.iter().any(|&m| m != 0) {
                        clipboard.bricks.insert([rel_bx, rel_by, rel_bz], src_brick);
                    }
                } else {
                    let mut out_brick = [0 as MaterialId; 64];
                    let mut has_content = false;
                    for lz_inner in 0..BRICK {
                        for ly_inner in 0..BRICK {
                            for lx_inner in 0..BRICK {
                                let wx = brick_origin.x + lx_inner as i32;
                                let wy = brick_origin.y + ly_inner as i32;
                                let wz = brick_origin.z + lz_inner as i32;

                                if wx < min.x
                                    || wx > max.x
                                    || wy < min.y
                                    || wy > max.y
                                    || wz < min.z
                                    || wz > max.z
                                {
                                    continue;
                                }

                                let bit = (lx_inner + ly_inner * BRICK + lz_inner * BRICK * BRICK)
                                    as usize;
                                let mat = src_brick[bit];
                                if sel.include_air || mat != 0 {
                                    out_brick[bit] = mat;
                                    if mat != 0 {
                                        has_content = true;
                                    }
                                }
                            }
                        }
                    }
                    if has_content || (sel.include_air && out_brick != [0; 64]) {
                        clipboard.bricks.insert([rel_bx, rel_by, rel_bz], out_brick);
                    }
                }
            }
        }
    }

    info!(
        "[selection] copied {} bricks, size={}x{}x{}",
        clipboard.bricks.len(),
        clipboard.size[0],
        clipboard.size[1],
        clipboard.size[2],
    );
}

fn clear_selection(
    sel: &SelectionState,
    editable: &mut EditableWorld,
    history: &mut EditHistory,
    dirty: &mut MeshDirty,
    pending: &mut PendingEdits,
) {
    let Some(min) = sel.aabb_min() else { return };
    let Some(max) = sel.aabb_max() else { return };

    let cxz = capy_world::CHUNK_XZ as i32;
    let cy = capy_world::CHUNK_Y as i32;
    let b = BRICK as i32;

    let wb_min = [
        min.x.div_euclid(b),
        min.y.div_euclid(b),
        min.z.div_euclid(b),
    ];
    let wb_max = [
        max.x.div_euclid(b),
        max.y.div_euclid(b),
        max.z.div_euclid(b),
    ];

    let mut changes = Vec::new();
    let mut pending_by_chunk: HashMap<[i32; 3], Vec<LeafBrickEdit>> = HashMap::new();

    for wbz in wb_min[2]..=wb_max[2] {
        for wby in wb_min[1]..=wb_max[1] {
            for wbx in wb_min[0]..=wb_max[0] {
                let brick_origin = IVec3::new(wbx * b, wby * b, wbz * b);
                let cc = [
                    brick_origin.x.div_euclid(cxz),
                    brick_origin.y.div_euclid(cy),
                    brick_origin.z.div_euclid(cxz),
                ];
                let lx = brick_origin.x.rem_euclid(cxz) as u32 / BRICK;
                let ly = brick_origin.y.rem_euclid(cy) as u32 / BRICK;
                let lz = brick_origin.z.rem_euclid(cxz) as u32 / BRICK;

                let chunk = editable.chunks.entry(cc).or_default();
                let old_brick = chunk.read_brick(lx, ly, lz);

                let brick_max = brick_origin + IVec3::splat(b - 1);
                let fully_inside = brick_origin.x >= min.x
                    && brick_origin.y >= min.y
                    && brick_origin.z >= min.z
                    && brick_max.x <= max.x
                    && brick_max.y <= max.y
                    && brick_max.z <= max.z;

                let new_brick = if fully_inside {
                    [0 as MaterialId; 64]
                } else {
                    let mut nb = old_brick;
                    for lz_inner in 0..BRICK {
                        for ly_inner in 0..BRICK {
                            for lx_inner in 0..BRICK {
                                let wx = brick_origin.x + lx_inner as i32;
                                let wy = brick_origin.y + ly_inner as i32;
                                let wz = brick_origin.z + lz_inner as i32;
                                if wx >= min.x
                                    && wx <= max.x
                                    && wy >= min.y
                                    && wy <= max.y
                                    && wz >= min.z
                                    && wz <= max.z
                                {
                                    let bit =
                                        (lx_inner + ly_inner * BRICK + lz_inner * BRICK * BRICK)
                                            as usize;
                                    nb[bit] = 0;
                                }
                            }
                        }
                    }
                    nb
                };

                if old_brick != new_brick {
                    chunk.write_brick(lx, ly, lz, new_brick);
                    changes.push(BrickChange {
                        chunk: cc,
                        brick: [lx, ly, lz],
                        old_materials: old_brick,
                        new_materials: new_brick,
                    });
                    pending_by_chunk.entry(cc).or_default().push(LeafBrickEdit {
                        bx: lx,
                        by: ly,
                        bz: lz,
                        materials: new_brick,
                    });
                }
            }
        }
    }

    if changes.is_empty() {
        return;
    }

    for (cc, edits) in pending_by_chunk {
        dirty.dirty.insert(cc);
        pending.by_chunk.entry(cc).or_default().extend(edits);
    }

    history.undo_stack.push(EditAction { changes });
    history.redo_stack.clear();

    info!("[selection] cleared selection to air");
}

#[allow(clippy::too_many_arguments)]
fn paste_clipboard(
    clipboard: &Clipboard,
    origin: IVec3,
    include_air: bool,
    grid_dim_xz: u32,
    editable: &mut EditableWorld,
    history: &mut EditHistory,
    dirty: &mut MeshDirty,
    pending: &mut PendingEdits,
) {
    if clipboard.is_empty() {
        return;
    }

    let cxz = capy_world::CHUNK_XZ as i32;
    let cy = capy_world::CHUNK_Y as i32;
    let half = (grid_dim_xz / 2) as i32;
    let min_x = -half * cxz;
    let max_x = (grid_dim_xz as i32 - half) * cxz - 1;
    let min_z = min_x;
    let max_z = max_x;
    let max_y = capy_world::CHUNK_Y as i32 - 1;

    let b = BRICK as i32;

    let mut staged_bricks: StagedBricks = HashMap::new();

    for (&rel_brick, &clip_brick) in &clipboard.bricks {
        let wb_origin = IVec3::new(
            origin.x + rel_brick[0] as i32 * b,
            origin.y + rel_brick[1] as i32 * b,
            origin.z + rel_brick[2] as i32 * b,
        );

        let wb_max = wb_origin + IVec3::splat(b - 1);
        if wb_max.x < min_x
            || wb_origin.x > max_x
            || wb_max.y < 0
            || wb_origin.y > max_y
            || wb_max.z < min_z
            || wb_origin.z > max_z
        {
            continue;
        }

        for lz in 0..BRICK {
            for ly in 0..BRICK {
                for lx in 0..BRICK {
                    let bit = (lx + ly * BRICK + lz * BRICK * BRICK) as usize;
                    let mat = clip_brick[bit];

                    if !include_air && mat == 0 {
                        continue;
                    }

                    let wx = wb_origin.x + lx as i32;
                    let wy = wb_origin.y + ly as i32;
                    let wz = wb_origin.z + lz as i32;

                    if wx < min_x || wx > max_x || wy < 0 || wy > max_y || wz < min_z || wz > max_z
                    {
                        continue;
                    }

                    let cc = [wx.div_euclid(cxz), wy.div_euclid(cy), wz.div_euclid(cxz)];
                    let dest_lx = wx.rem_euclid(cxz) as u32;
                    let dest_ly = wy.rem_euclid(cy) as u32;
                    let dest_lz = wz.rem_euclid(cxz) as u32;
                    let dest_brick = [dest_lx / BRICK, dest_ly / BRICK, dest_lz / BRICK];
                    let dest_bit = ((dest_lx % BRICK)
                        + (dest_ly % BRICK) * BRICK
                        + (dest_lz % BRICK) * BRICK * BRICK)
                        as usize;

                    let entry = staged_bricks.entry((cc, dest_brick)).or_insert_with(|| {
                        let old = editable.chunks.get(&cc).map_or_else(
                            || {
                                EditableChunk::default().read_brick(
                                    dest_brick[0],
                                    dest_brick[1],
                                    dest_brick[2],
                                )
                            },
                            |chunk| chunk.read_brick(dest_brick[0], dest_brick[1], dest_brick[2]),
                        );
                        (old, old)
                    });
                    entry.1[dest_bit] = mat;
                }
            }
        }
    }

    if staged_bricks.is_empty() {
        return;
    }

    let mut changes = Vec::new();
    let mut pending_by_chunk: HashMap<[i32; 3], Vec<LeafBrickEdit>> = HashMap::new();

    for ((cc, brick_coord), (old_brick, new_brick)) in staged_bricks {
        if old_brick == new_brick {
            continue;
        }

        let chunk = editable.chunks.entry(cc).or_default();
        chunk.write_brick(brick_coord[0], brick_coord[1], brick_coord[2], new_brick);

        changes.push(BrickChange {
            chunk: cc,
            brick: brick_coord,
            old_materials: old_brick,
            new_materials: new_brick,
        });
        pending_by_chunk.entry(cc).or_default().push(LeafBrickEdit {
            bx: brick_coord[0],
            by: brick_coord[1],
            bz: brick_coord[2],
            materials: new_brick,
        });
    }

    if changes.is_empty() {
        return;
    }

    for (cc, edits) in pending_by_chunk {
        dirty.dirty.insert(cc);
        pending.by_chunk.entry(cc).or_default().extend(edits);
    }

    history.undo_stack.push(EditAction { changes });
    history.redo_stack.clear();

    info!(
        "[selection] pasted clipboard at ({}, {}, {})",
        origin.x, origin.y, origin.z
    );
}

// ---------------------------------------------------------------------------
// In-place rotation
// ---------------------------------------------------------------------------

fn rotate_selection_in_place(
    sel: &mut SelectionState,
    editable: &mut EditableWorld,
    history: &mut EditHistory,
    dirty: &mut MeshDirty,
    pending: &mut PendingEdits,
) {
    let Some(min) = sel.aabb_min() else { return };
    let Some(max) = sel.aabb_max() else { return };

    let dims = max - min + IVec3::ONE;
    let sx = dims.x as u32;
    let sy = dims.y as u32;
    let sz = dims.z as u32;

    // 1. Copy selection into a temp clipboard (include air = true)
    let mut temp = Clipboard {
        size: [sx, sy, sz],
        ..Clipboard::default()
    };

    let cxz = capy_world::CHUNK_XZ as i32;
    let cy = capy_world::CHUNK_Y as i32;
    let b = BRICK as i32;

    let wb_min = [
        min.x.div_euclid(b),
        min.y.div_euclid(b),
        min.z.div_euclid(b),
    ];
    let wb_max = [
        max.x.div_euclid(b),
        max.y.div_euclid(b),
        max.z.div_euclid(b),
    ];

    let default_chunk = EditableChunk::default();

    for wbz in wb_min[2]..=wb_max[2] {
        for wby in wb_min[1]..=wb_max[1] {
            for wbx in wb_min[0]..=wb_max[0] {
                let brick_origin = IVec3::new(wbx * b, wby * b, wbz * b);
                let cc = [
                    brick_origin.x.div_euclid(cxz),
                    brick_origin.y.div_euclid(cy),
                    brick_origin.z.div_euclid(cxz),
                ];
                let lx = brick_origin.x.rem_euclid(cxz) as u32 / BRICK;
                let ly = brick_origin.y.rem_euclid(cy) as u32 / BRICK;
                let lz = brick_origin.z.rem_euclid(cxz) as u32 / BRICK;

                let chunk = editable.chunks.get(&cc).unwrap_or(&default_chunk);
                let src_brick = chunk.read_brick(lx, ly, lz);

                let rel_bx = (wbx - wb_min[0]) as u32;
                let rel_by = (wby - wb_min[1]) as u32;
                let rel_bz = (wbz - wb_min[2]) as u32;

                let brick_max = brick_origin + IVec3::splat(b - 1);
                let fully_inside = brick_origin.x >= min.x
                    && brick_origin.y >= min.y
                    && brick_origin.z >= min.z
                    && brick_max.x <= max.x
                    && brick_max.y <= max.y
                    && brick_max.z <= max.z;

                if fully_inside {
                    temp.bricks.insert([rel_bx, rel_by, rel_bz], src_brick);
                } else {
                    // Partial brick — mask to only voxels inside the selection
                    let mut out_brick = [0 as MaterialId; 64];
                    for lz_inner in 0..BRICK {
                        for ly_inner in 0..BRICK {
                            for lx_inner in 0..BRICK {
                                let wx = brick_origin.x + lx_inner as i32;
                                let wy = brick_origin.y + ly_inner as i32;
                                let wz = brick_origin.z + lz_inner as i32;
                                if wx >= min.x
                                    && wx <= max.x
                                    && wy >= min.y
                                    && wy <= max.y
                                    && wz >= min.z
                                    && wz <= max.z
                                {
                                    let bit =
                                        (lx_inner + ly_inner * BRICK + lz_inner * BRICK * BRICK)
                                            as usize;
                                    out_brick[bit] = src_brick[bit];
                                }
                            }
                        }
                    }
                    temp.bricks.insert([rel_bx, rel_by, rel_bz], out_brick);
                }
            }
        }
    }

    // 2. Rotate the temp clipboard
    temp.rotate_cw_y();

    // 3. Compute new AABB: center stays fixed, X and Z dims swap
    let center_2x = min + max; // 2 * center (avoid float)
    let new_sx = sz as i32;
    let new_sz = sx as i32;
    let new_min_x = (center_2x.x - new_sx + 1).div_euclid(2);
    let new_min_z = (center_2x.z - new_sz + 1).div_euclid(2);
    let new_min = IVec3::new(new_min_x, min.y, new_min_z);
    let new_max = new_min + IVec3::new(new_sx - 1, dims.y - 1, new_sz - 1);

    // 4. Snapshot all bricks in the union of old and new bounds
    let union_min = min.min(new_min);
    let union_max = max.max(new_max);

    let uwb_min = [
        union_min.x.div_euclid(b),
        union_min.y.div_euclid(b),
        union_min.z.div_euclid(b),
    ];
    let uwb_max = [
        union_max.x.div_euclid(b),
        union_max.y.div_euclid(b),
        union_max.z.div_euclid(b),
    ];

    // staged: (chunk_coord, brick_coord) → (old_brick, new_brick)
    let mut staged: StagedBricks = HashMap::new();

    // Snapshot old state for all bricks in the union
    for wbz in uwb_min[2]..=uwb_max[2] {
        for wby in uwb_min[1]..=uwb_max[1] {
            for wbx in uwb_min[0]..=uwb_max[0] {
                let brick_origin = IVec3::new(wbx * b, wby * b, wbz * b);
                let cc = [
                    brick_origin.x.div_euclid(cxz),
                    brick_origin.y.div_euclid(cy),
                    brick_origin.z.div_euclid(cxz),
                ];
                let blx = brick_origin.x.rem_euclid(cxz) as u32 / BRICK;
                let bly = brick_origin.y.rem_euclid(cy) as u32 / BRICK;
                let blz = brick_origin.z.rem_euclid(cxz) as u32 / BRICK;

                let chunk = editable.chunks.get(&cc).unwrap_or(&default_chunk);
                let old = chunk.read_brick(blx, bly, blz);
                staged.entry((cc, [blx, bly, blz])).or_insert((old, old));
            }
        }
    }

    // 5. Clear old selection voxels to air in staged map
    for wbz in wb_min[2]..=wb_max[2] {
        for wby in wb_min[1]..=wb_max[1] {
            for wbx in wb_min[0]..=wb_max[0] {
                let brick_origin = IVec3::new(wbx * b, wby * b, wbz * b);
                let cc = [
                    brick_origin.x.div_euclid(cxz),
                    brick_origin.y.div_euclid(cy),
                    brick_origin.z.div_euclid(cxz),
                ];
                let blx = brick_origin.x.rem_euclid(cxz) as u32 / BRICK;
                let bly = brick_origin.y.rem_euclid(cy) as u32 / BRICK;
                let blz = brick_origin.z.rem_euclid(cxz) as u32 / BRICK;

                let entry = staged.entry((cc, [blx, bly, blz]));
                let (_, new_brick) = entry.or_insert_with(|| {
                    let chunk = editable.chunks.get(&cc).unwrap_or(&default_chunk);
                    let old = chunk.read_brick(blx, bly, blz);
                    (old, old)
                });

                let brick_max = brick_origin + IVec3::splat(b - 1);
                let fully_inside = brick_origin.x >= min.x
                    && brick_origin.y >= min.y
                    && brick_origin.z >= min.z
                    && brick_max.x <= max.x
                    && brick_max.y <= max.y
                    && brick_max.z <= max.z;

                if fully_inside {
                    *new_brick = [0 as MaterialId; 64];
                } else {
                    for lz_inner in 0..BRICK {
                        for ly_inner in 0..BRICK {
                            for lx_inner in 0..BRICK {
                                let wx = brick_origin.x + lx_inner as i32;
                                let wy = brick_origin.y + ly_inner as i32;
                                let wz = brick_origin.z + lz_inner as i32;
                                if wx >= min.x
                                    && wx <= max.x
                                    && wy >= min.y
                                    && wy <= max.y
                                    && wz >= min.z
                                    && wz <= max.z
                                {
                                    let bit =
                                        (lx_inner + ly_inner * BRICK + lz_inner * BRICK * BRICK)
                                            as usize;
                                    new_brick[bit] = 0;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // 6. Write rotated clipboard voxels into new bounds
    for (&rel_brick, clip_data) in &temp.bricks {
        for lz in 0..BRICK {
            for ly in 0..BRICK {
                for lx in 0..BRICK {
                    let bit = (lx + ly * BRICK + lz * BRICK * BRICK) as usize;
                    let mat = clip_data[bit];

                    // Voxel coord relative to new_min
                    let vx = rel_brick[0] * BRICK + lx;
                    let vy = rel_brick[1] * BRICK + ly;
                    let vz = rel_brick[2] * BRICK + lz;

                    if vx >= temp.size[0] || vy >= temp.size[1] || vz >= temp.size[2] {
                        continue;
                    }

                    // World coord
                    let wx = new_min.x + vx as i32;
                    let wy = new_min.y + vy as i32;
                    let wz = new_min.z + vz as i32;

                    let cc = [wx.div_euclid(cxz), wy.div_euclid(cy), wz.div_euclid(cxz)];
                    let dest_lx = wx.rem_euclid(cxz) as u32;
                    let dest_ly = wy.rem_euclid(cy) as u32;
                    let dest_lz = wz.rem_euclid(cxz) as u32;
                    let dest_brick_coord = [dest_lx / BRICK, dest_ly / BRICK, dest_lz / BRICK];
                    let dest_bit = ((dest_lx % BRICK)
                        + (dest_ly % BRICK) * BRICK
                        + (dest_lz % BRICK) * BRICK * BRICK)
                        as usize;

                    let entry = staged.entry((cc, dest_brick_coord)).or_insert_with(|| {
                        let chunk = editable.chunks.get(&cc).unwrap_or(&default_chunk);
                        let old = chunk.read_brick(
                            dest_brick_coord[0],
                            dest_brick_coord[1],
                            dest_brick_coord[2],
                        );
                        (old, old)
                    });
                    entry.1[dest_bit] = mat;
                }
            }
        }
    }

    // 7. Commit all changed bricks
    let mut changes = Vec::new();
    let mut pending_by_chunk: HashMap<[i32; 3], Vec<LeafBrickEdit>> = HashMap::new();

    for ((cc, brick_coord), (old_brick, new_brick)) in staged {
        if old_brick == new_brick {
            continue;
        }

        let chunk = editable.chunks.entry(cc).or_default();
        chunk.write_brick(brick_coord[0], brick_coord[1], brick_coord[2], new_brick);

        changes.push(BrickChange {
            chunk: cc,
            brick: brick_coord,
            old_materials: old_brick,
            new_materials: new_brick,
        });
        pending_by_chunk.entry(cc).or_default().push(LeafBrickEdit {
            bx: brick_coord[0],
            by: brick_coord[1],
            bz: brick_coord[2],
            materials: new_brick,
        });
    }

    if changes.is_empty() {
        info!("[selection] rotation produced no changes");
        return;
    }

    for (cc, edits) in pending_by_chunk {
        dirty.dirty.insert(cc);
        pending.by_chunk.entry(cc).or_default().extend(edits);
    }

    history.undo_stack.push(EditAction { changes });
    history.redo_stack.clear();

    // Update selection corners to new bounds
    sel.corner_a = Some(new_min);
    sel.corner_b = Some(new_max);

    info!(
        "[selection] rotated in-place, new bounds ({},{},{})..({},{},{})",
        new_min.x, new_min.y, new_min.z, new_max.x, new_max.y, new_max.z,
    );
}
