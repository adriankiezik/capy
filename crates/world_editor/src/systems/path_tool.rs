use std::collections::HashMap;
use std::sync::mpsc;
use std::time::Instant;

use bevy_ecs::system::{NonSendMut, Res, ResMut};
use capy_core::{KeyCode, MaterialId, MouseButton};
use capy_world::LeafBrickEdit;
use glam::Vec3;
use tracing::info;

use crate::resources::path_state::{PathMode, PathState};
use crate::resources::{
    BrickChange, EditAction, EditHistory, EditTask, EditTaskOutput, EditableChunk, EditableWorld,
    EditorState, EditorTool, InputEdge, MeshDirty, PendingEdits, UpdatedChunk, VoxelHit,
};

const BRICK: u32 = 4;

// ---------------------------------------------------------------------------
// Position hash & color jitter (mirrored from edit_apply)
// ---------------------------------------------------------------------------

#[inline]
fn position_hash(x: i32, y: i32, z: i32) -> f32 {
    let mut h = (x as u32)
        .wrapping_mul(374_761_393)
        .wrapping_add((y as u32).wrapping_mul(668_265_263))
        .wrapping_add((z as u32).wrapping_mul(1_274_126_177));
    h = (h ^ (h >> 13)).wrapping_mul(1_103_515_245);
    h = h ^ (h >> 16);
    (h & 0xFFFF) as f32 / 65536.0
}

fn build_jitter_palette(base_material: MaterialId, jitter: f32) -> Vec<MaterialId> {
    if jitter <= 0.0 {
        return vec![base_material];
    }
    let base_color = capy_core::MATERIAL_COLORS[base_material as usize];
    let steps = 16usize;
    let mut palette = Vec::with_capacity(steps);
    for i in 0..steps {
        let t = i as f32 / steps as f32;
        let angle = t * std::f32::consts::TAU;
        let (s, c) = angle.sin_cos();
        let jr = jitter * 0.3 * c;
        let jg = jitter * 0.3 * s;
        let jb = jitter * 0.3 * (c + s) * 0.5;
        let color = [
            (base_color[0] + jr).clamp(0.0, 1.0),
            (base_color[1] + jg).clamp(0.0, 1.0),
            (base_color[2] + jb).clamp(0.0, 1.0),
        ];
        palette.push(capy_core::closest_material(color));
    }
    palette
}

// ---------------------------------------------------------------------------
// Catmull-Rom spline
// ---------------------------------------------------------------------------

/// Evaluate a Catmull-Rom spline segment at parameter `t` in [0, 1].
fn catmull_rom(p0: Vec3, p1: Vec3, p2: Vec3, p3: Vec3, t: f32) -> Vec3 {
    let t2 = t * t;
    let t3 = t2 * t;
    0.5 * ((2.0 * p1)
        + (-p0 + p2) * t
        + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2
        + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * t3)
}

/// Sample a full Catmull-Rom spline through the given waypoints.
/// Returns a dense list of points spaced ~1 voxel apart.
fn sample_spline(waypoints: &[Vec3]) -> Vec<Vec3> {
    let n = waypoints.len();
    if n < 2 {
        return waypoints.to_vec();
    }

    let mut samples = Vec::new();

    for i in 0..(n - 1) {
        // Phantom control points at endpoints: duplicate first/last.
        let p0 = if i == 0 {
            waypoints[0]
        } else {
            waypoints[i - 1]
        };
        let p1 = waypoints[i];
        let p2 = waypoints[i + 1];
        let p3 = if i + 2 < n {
            waypoints[i + 2]
        } else {
            waypoints[n - 1]
        };

        let segment_len = (p2 - p1).length();
        // At least 1 sample per voxel of segment length.
        let steps = (segment_len.ceil() as u32).max(2);

        for s in 0..steps {
            let t = s as f32 / steps as f32;
            samples.push(catmull_rom(p0, p1, p2, p3, t));
        }
    }
    // Always include the last waypoint.
    if let Some(&last) = waypoints.last() {
        samples.push(last);
    }
    samples
}

// ---------------------------------------------------------------------------
// Path rasterization (spline → voxel edits)
// ---------------------------------------------------------------------------

/// Rasterize a path from spline samples into voxel edits.
///
/// For each sample, we paint a strip of `half_width` voxels perpendicular to
/// the path direction. We use a simple approach: for each sample, iterate
/// all (x,z) within a square of side `2*half_width+1` centered on the sample,
/// and keep only those within distance `half_width` of the nearest spline point.
///
/// This is called on a background thread.
fn rasterize_path(
    samples: &[Vec3],
    half_width: u32,
    mode: PathMode,
    material: MaterialId,
    color_jitter: f32,
    mut chunks: HashMap<[i32; 3], EditableChunk>,
) -> EditTaskOutput {
    let t_total = Instant::now();

    // Build a set of all (wx, wz) columns the path touches, with the
    // interpolated target height at that column.
    let mut column_targets: HashMap<(i32, i32), f32> = HashMap::new();
    let hw = half_width as i32;

    for sample in samples {
        let cx = sample.x.floor() as i32;
        let cz = sample.z.floor() as i32;
        let sy = sample.y;

        for dz in -hw..=hw {
            for dx in -hw..=hw {
                if dx * dx + dz * dz > hw * hw {
                    continue; // circular footprint
                }
                let wx = cx + dx;
                let wz = cz + dz;
                // Use the sample closest to the center for height target.
                // If already set, average with existing (smooth blending).
                column_targets
                    .entry((wx, wz))
                    .and_modify(|h| *h = (*h + sy) * 0.5)
                    .or_insert(sy);
            }
        }
    }

    let t_loop = Instant::now();

    let jitter_palette = build_jitter_palette(material, color_jitter);
    let mut original_bricks: HashMap<([i32; 3], [u32; 3]), [MaterialId; 64]> = HashMap::new();

    for (&(wx, wz), &target_y) in &column_targets {
        let (cc, lx, lz) = world_to_chunk_local(wx, wz);
        let chunk = chunks.entry(cc).or_default();

        // Pick a jittered material for this column.
        let mat = jitter_palette[(position_hash(wx, 0, wz) * jitter_palette.len() as f32) as usize
            % jitter_palette.len()];

        match mode {
            PathMode::Paint => {
                // Just repaint the topmost solid voxel.
                if let Some(surface_y) = find_surface_height(chunk, lx, lz) {
                    set_voxel(chunk, cc, lx, surface_y, lz, mat, &mut original_bricks);
                }
            }
            PathMode::Flatten => {
                let current_h = find_surface_height(chunk, lx, lz);
                let target_h = target_y.floor() as u32;

                // Modify column height to match the interpolated path height.
                modify_column_height(
                    chunk,
                    lx,
                    lz,
                    current_h,
                    Some(target_h),
                    mat,
                    cc,
                    &mut original_bricks,
                );

                // Always paint the surface so the path is visible even on
                // already-flat terrain.
                set_voxel(chunk, cc, lx, target_h, lz, mat, &mut original_bricks);
            }
        }
    }

    let loop_ms = t_loop.elapsed().as_secs_f64() * 1000.0;

    // Convert original_bricks → BrickChanges + UpdatedChunks.
    let mut changes = Vec::with_capacity(original_bricks.len());
    let mut changed_chunks: HashMap<[i32; 3], Vec<LeafBrickEdit>> = HashMap::new();

    for ((cc, brick), old_materials) in &original_bricks {
        let chunk = chunks.entry(*cc).or_default();
        let new_materials = chunk.read_brick(brick[0], brick[1], brick[2]);
        if *old_materials == new_materials {
            continue;
        }
        changes.push(BrickChange {
            chunk: *cc,
            brick: *brick,
            old_materials: *old_materials,
            new_materials,
        });
        changed_chunks.entry(*cc).or_default().push(LeafBrickEdit {
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
        radius: half_width as i32,
    }
}

// ---------------------------------------------------------------------------
// Helpers (duplicated from edit_apply to avoid pub(crate) coupling)
// ---------------------------------------------------------------------------

#[inline]
fn world_to_chunk_local(wx: i32, wz: i32) -> ([i32; 3], u32, u32) {
    let cxz = capy_world::CHUNK_XZ as i32;
    let ccx = wx.div_euclid(cxz);
    let ccz = wz.div_euclid(cxz);
    let lx = wx.rem_euclid(cxz) as u32;
    let lz = wz.rem_euclid(cxz) as u32;
    ([ccx, 0, ccz], lx, lz)
}

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

/// Set a single voxel's material, tracking the original brick for undo.
fn set_voxel(
    chunk: &mut EditableChunk,
    cc: [i32; 3],
    lx: u32,
    ly: u32,
    lz: u32,
    material: MaterialId,
    original_bricks: &mut HashMap<([i32; 3], [u32; 3]), [MaterialId; 64]>,
) {
    let bx = lx / BRICK;
    let by = ly / BRICK;
    let bz = lz / BRICK;
    let local_x = lx % BRICK;
    let local_y = ly % BRICK;
    let local_z = lz % BRICK;
    let idx = (local_x + local_y * BRICK + local_z * BRICK * BRICK) as usize;

    let brick_key = (cc, [bx, by, bz]);
    original_bricks
        .entry(brick_key)
        .or_insert_with(|| chunk.read_brick(bx, by, bz));

    let mut brick = chunk.read_brick(bx, by, bz);
    if brick[idx] != 0 {
        brick[idx] = material;
        chunk.write_brick(bx, by, bz, brick);
    }
}

/// Modify a column's height (fill or clear voxels between current and target).
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

    let cur_fill: i32 = current_h.map_or(-1, |h| h as i32);
    let tgt_fill: i32 = target_h.map_or(-1, |h| h as i32);

    if cur_fill == tgt_fill {
        return;
    }

    let bx = lx / BRICK;
    let bz = lz / BRICK;
    let local_x = lx % BRICK;
    let local_z = lz % BRICK;

    let (y_lo, y_hi, fill_mat) = if tgt_fill > cur_fill {
        ((cur_fill + 1) as u32, tgt_fill as u32, material)
    } else {
        ((tgt_fill + 1) as u32, cur_fill as u32, 0 as MaterialId)
    };

    let by_lo = y_lo / BRICK;
    let by_hi = y_hi / BRICK;

    for by in by_lo..=by_hi {
        let brick_key = (cc, [bx, by, bz]);
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

fn snapshot_affected_chunks(
    editable: &EditableWorld,
    cc_min: [i32; 3],
    cc_max: [i32; 3],
) -> HashMap<[i32; 3], EditableChunk> {
    let mut chunks = HashMap::new();
    for ccz in cc_min[2]..=cc_max[2] {
        for ccx in cc_min[0]..=cc_max[0] {
            let cc = [ccx, 0, ccz];
            if let Some(chunk) = editable.chunks.get(&cc) {
                chunks.insert(cc, chunk.clone());
            }
        }
    }
    chunks
}

// ---------------------------------------------------------------------------
// apply_edit_output — same as edit_apply.rs version
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
        "[path_tool] total={:.1}ms | worker={worker_ms:.1}ms, loop={loop_ms:.1}ms, \
         apply={apply_ms:.1}ms | bricks={num_changes}, chunks={num_chunks}, radius={radius}",
        worker_ms + apply_ms
    );
}

// ---------------------------------------------------------------------------
// Path tool ECS system
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
pub(crate) fn path_tool(
    edge: Res<InputEdge>,
    voxel_hit: Option<Res<VoxelHit>>,
    state: Res<EditorState>,
    mut path_state: ResMut<PathState>,
    mut editable: ResMut<EditableWorld>,
    mut history: ResMut<EditHistory>,
    mut dirty: ResMut<MeshDirty>,
    mut pending: ResMut<PendingEdits>,
    mut task: NonSendMut<EditTask>,
) {
    // Only active when Path tool is selected.
    if state.active_tool != EditorTool::Path {
        // If user switches away from path tool, clear waypoints.
        if !path_state.waypoints.is_empty() {
            path_state.waypoints.clear();
            path_state.dirty = false;
            path_state.confirmed = false;
        }
        return;
    }

    // Drain background result if ready.
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
                tracing::error!("[path_tool] background worker disconnected");
            }
        }
    }

    // --- Backspace: remove last waypoint ---
    if edge.keys_just_pressed.contains(&KeyCode::Backspace) && !path_state.waypoints.is_empty() {
        path_state.waypoints.pop();
        path_state.dirty = true;
    }

    // --- Escape: cancel path ---
    if edge.keys_just_pressed.contains(&KeyCode::Escape) {
        path_state.waypoints.clear();
        path_state.dirty = true;
        path_state.confirmed = false;
        return;
    }

    // --- Enter/Return: confirm path ---
    if edge.keys_just_pressed.contains(&KeyCode::Enter) && path_state.waypoints.len() >= 2 {
        path_state.confirmed = true;
    }

    // --- Left click: place a waypoint ---
    if edge.mouse_just_pressed.contains(&MouseButton::Left) {
        if let Some(hit) = &voxel_hit {
            if hit.hit {
                let p = hit.position - hit.normal * 0.5;
                let snap = Vec3::new(p.x.floor() + 0.5, p.y.floor(), p.z.floor() + 0.5);
                path_state.waypoints.push(snap);
                path_state.dirty = true;
            }
        }
    }

    // --- Confirm: compute and apply the path ---
    if path_state.confirmed && path_state.waypoints.len() >= 2 {
        let waypoints = path_state.waypoints.clone();
        let half_width = path_state.path_width;
        let mode = path_state.mode;
        let material = state.selected_material;
        let color_jitter = state.color_jitter;

        // Compute AABB of all waypoints + width margin to snapshot chunks.
        let margin = half_width as f32 + 2.0;
        let (mut min_x, mut min_z) = (f32::MAX, f32::MAX);
        let (mut max_x, mut max_z) = (f32::MIN, f32::MIN);
        for wp in &waypoints {
            min_x = min_x.min(wp.x);
            min_z = min_z.min(wp.z);
            max_x = max_x.max(wp.x);
            max_z = max_z.max(wp.z);
        }
        let cxz = capy_world::CHUNK_XZ as i32;
        let cc_min = [
            ((min_x - margin).floor() as i32).div_euclid(cxz),
            0,
            ((min_z - margin).floor() as i32).div_euclid(cxz),
        ];
        let cc_max = [
            ((max_x + margin).ceil() as i32).div_euclid(cxz),
            0,
            ((max_z + margin).ceil() as i32).div_euclid(cxz),
        ];

        let chunks = snapshot_affected_chunks(&editable, cc_min, cc_max);

        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let samples = sample_spline(&waypoints);
            let output = rasterize_path(&samples, half_width, mode, material, color_jitter, chunks);
            let _ = tx.send(output);
        });

        task.pending = Some(rx);

        // Clear state after confirming.
        path_state.waypoints.clear();
        path_state.confirmed = false;
        path_state.dirty = false;
    }
}

// ---------------------------------------------------------------------------
// Preview: draw the spline as a screen-space overlay
// ---------------------------------------------------------------------------

pub(crate) fn draw_path_preview(
    ctx: &egui::Context,
    camera: &capy_core::Camera,
    window: &capy_core::GameWindow,
    path_state: &PathState,
) {
    if path_state.waypoints.is_empty() {
        return;
    }

    let w = window.width as f32;
    let h = window.height as f32;
    if w < 1.0 || h < 1.0 {
        return;
    }

    let view = glam::Mat4::look_to_rh(camera.position, camera.forward(), Vec3::Y);
    let proj = glam::Mat4::perspective_infinite_rh(camera.fov_y, camera.aspect, 0.1);
    let vp = proj * view;

    let mut painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("path_preview"),
    ));
    painter.set_clip_rect(ctx.content_rect());

    // Draw spline curve if >= 2 waypoints.
    if path_state.waypoints.len() >= 2 {
        let samples = sample_spline(&path_state.waypoints);
        let projected: Vec<Option<egui::Pos2>> = samples
            .iter()
            .map(|&p| project_to_screen(&vp, w, h, p))
            .collect();

        // Draw path as a polyline.
        let stroke = egui::Stroke::new(2.5, egui::Color32::from_rgb(255, 200, 50));
        for pair in projected.windows(2) {
            if let (Some(a), Some(b)) = (pair[0], pair[1]) {
                painter.line_segment([a, b], stroke);
            }
        }

        // Draw path width indicators at every Nth sample.
        let width_stroke = egui::Stroke::new(
            1.0,
            egui::Color32::from_rgba_unmultiplied(255, 200, 50, 100),
        );
        let half_w = path_state.path_width as f32;
        let step = (samples.len() / 20).max(1);
        for i in (0..samples.len()).step_by(step) {
            let p = samples[i];
            // Approximate perpendicular in XZ plane.
            let tangent = if i + 1 < samples.len() {
                (samples[i + 1] - p).normalize_or_zero()
            } else if i > 0 {
                (p - samples[i - 1]).normalize_or_zero()
            } else {
                Vec3::X
            };
            let perp = Vec3::new(-tangent.z, 0.0, tangent.x);
            let left = p + perp * half_w;
            let right = p - perp * half_w;
            if let (Some(sl), Some(sr)) = (
                project_to_screen(&vp, w, h, left),
                project_to_screen(&vp, w, h, right),
            ) {
                painter.line_segment([sl, sr], width_stroke);
            }
        }
    }

    // Draw waypoint markers.
    let dot_radius = 5.0;
    for (i, &wp) in path_state.waypoints.iter().enumerate() {
        if let Some(sp) = project_to_screen(&vp, w, h, wp) {
            let color = if i == 0 {
                egui::Color32::from_rgb(100, 255, 100) // green start
            } else {
                egui::Color32::from_rgb(255, 200, 50) // gold
            };
            painter.circle_filled(sp, dot_radius, color);
            painter.circle_stroke(sp, dot_radius, egui::Stroke::new(1.5, egui::Color32::WHITE));
        }
    }
}

fn project_to_screen(vp: &glam::Mat4, w: f32, h: f32, point: Vec3) -> Option<egui::Pos2> {
    let clip = *vp * glam::Vec4::new(point.x, point.y, point.z, 1.0);
    if clip.w < 0.1 {
        return None;
    }
    let ndc = clip.truncate() / clip.w;
    let margin = 4000.0;
    let sx = ((ndc.x * 0.5 + 0.5) * w).clamp(-margin, w + margin);
    let sy = ((1.0 - (ndc.y * 0.5 + 0.5)) * h).clamp(-margin, h + margin);
    Some(egui::pos2(sx, sy))
}
