use bevy_ecs::system::{Res, ResMut};
use capy_core::{Camera, GameWindow, MATERIAL_COLORS};
use capy_ui::EguiContext;
use glam::{Mat4, Vec3, Vec4};

use crate::resources::path_state::{PathMode, PathState};
use crate::resources::{
    BrushShape, EditorState, EditorTool, Face, FoliageAction, FoliageMode, MaskMode, PrefabLibrary,
    SaveResult, SaveState, SelectionPhase, SelectionState, WaterAction,
};

#[allow(clippy::too_many_arguments)]
pub(crate) fn editor_ui(
    egui_ctx: Res<EguiContext>,
    mut state: ResMut<EditorState>,
    mut prefabs: ResMut<PrefabLibrary>,
    mut sel: ResMut<SelectionState>,
    camera: Res<Camera>,
    window: Res<GameWindow>,
    mut save_state: ResMut<SaveState>,
    mut path_state: ResMut<PathState>,
) {
    let mut selected_prefab = None;
    let mut regenerate_selected = None;

    egui::Window::new("Tools")
        .default_open(true)
        .resizable(false)
        .show(&egui_ctx.0, |ui| {
            if save_state.requested {
                ui.colored_label(egui::Color32::YELLOW, "Waiting for bake...");
            } else if let Some((when, result)) = &save_state.last_save
                && when.elapsed().as_secs() < 5
            {
                match result {
                    SaveResult::Success(count) => {
                        ui.colored_label(
                            egui::Color32::GREEN,
                            format!("Saved! ({count} edited chunks)"),
                        );
                    }
                    SaveResult::Error(msg) => {
                        ui.colored_label(egui::Color32::RED, format!("Error: {msg}"));
                    }
                }
            }
            ui.label("Tool");
            ui.horizontal(|ui| {
                ui.selectable_value(&mut state.active_tool, EditorTool::Place, "Place (1)");
                ui.selectable_value(&mut state.active_tool, EditorTool::Remove, "Remove (2)");
                ui.selectable_value(&mut state.active_tool, EditorTool::Paint, "Paint (3)");
            });
            ui.horizontal(|ui| {
                ui.selectable_value(&mut state.active_tool, EditorTool::Raise, "Raise (4)");
                ui.selectable_value(&mut state.active_tool, EditorTool::Lower, "Lower (5)");
                ui.selectable_value(&mut state.active_tool, EditorTool::Smooth, "Smooth (6)");
            });
            ui.horizontal(|ui| {
                ui.selectable_value(&mut state.active_tool, EditorTool::Prefab, "Prefab (7)");
                ui.selectable_value(&mut state.active_tool, EditorTool::Select, "Select (8)");
                ui.selectable_value(&mut state.active_tool, EditorTool::Foliage, "Foliage (9)");
            });
            ui.horizontal(|ui| {
                ui.selectable_value(&mut state.active_tool, EditorTool::Water, "Water (0)");
                ui.selectable_value(
                    &mut state.active_tool,
                    EditorTool::ColorPick,
                    "Color Pick (-)",
                );
                ui.selectable_value(&mut state.active_tool, EditorTool::Path, "Path (P)");
            });
            ui.separator();

            if state.active_tool == EditorTool::ColorPick {
                ui.label("Click a voxel to pick its color.");
                let mat = state.selected_material;
                let matched = MATERIAL_COLORS[mat as usize];
                let matched_color = egui::Color32::from_rgb(
                    (matched[0] * 255.0) as u8,
                    (matched[1] * 255.0) as u8,
                    (matched[2] * 255.0) as u8,
                );
                ui.horizontal(|ui| {
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::hover());
                    ui.painter().rect_filled(rect, 2.0, matched_color);
                    ui.label(format!("Palette #{mat}"));
                });
                ui.separator();
            }

            if state.active_tool == EditorTool::Path {
                ui.label("Color");
                let mut color = egui::Color32::from_rgb(
                    state.picked_color[0],
                    state.picked_color[1],
                    state.picked_color[2],
                );
                let response = egui::color_picker::color_edit_button_srgba(
                    ui,
                    &mut color,
                    egui::color_picker::Alpha::Opaque,
                );
                if response.changed() {
                    state.picked_color = [color.r(), color.g(), color.b()];
                    let color_f32 = [
                        color.r() as f32 / 255.0,
                        color.g() as f32 / 255.0,
                        color.b() as f32 / 255.0,
                    ];
                    state.selected_material = capy_core::closest_material(color_f32);
                }
                let mat = state.selected_material;
                let matched = MATERIAL_COLORS[mat as usize];
                let matched_color = egui::Color32::from_rgb(
                    (matched[0] * 255.0) as u8,
                    (matched[1] * 255.0) as u8,
                    (matched[2] * 255.0) as u8,
                );
                ui.horizontal(|ui| {
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::hover());
                    ui.painter().rect_filled(rect, 2.0, matched_color);
                    ui.label(format!("Palette #{mat}"));
                });
                ui.label("Color Jitter");
                ui.add(egui::Slider::new(&mut state.color_jitter, 0.0..=1.0).fixed_decimals(2));
                ui.separator();
            }

            if state.active_tool == EditorTool::Select {
                if let Some(dims) = sel.dimensions() {
                    ui.label(format!("{} x {} x {} voxels", dims.x, dims.y, dims.z));
                }
                ui.checkbox(&mut sel.include_air, "Include air");
                ui.checkbox(&mut sel.move_voxels, "Move voxels");

                ui.separator();
            }

            let is_brush_tool = !matches!(
                state.active_tool,
                EditorTool::Prefab | EditorTool::Select | EditorTool::ColorPick | EditorTool::Path
            );
            if is_brush_tool {
                // --- Color (before brush size) ---
                if !matches!(state.active_tool, EditorTool::Foliage | EditorTool::Water) {
                    ui.label("Color");
                    let mut color = egui::Color32::from_rgb(
                        state.picked_color[0],
                        state.picked_color[1],
                        state.picked_color[2],
                    );
                    let response = egui::color_picker::color_edit_button_srgba(
                        ui,
                        &mut color,
                        egui::color_picker::Alpha::Opaque,
                    );
                    if response.changed() {
                        state.picked_color = [color.r(), color.g(), color.b()];
                        let color_f32 = [
                            color.r() as f32 / 255.0,
                            color.g() as f32 / 255.0,
                            color.b() as f32 / 255.0,
                        ];
                        state.selected_material = capy_core::closest_material(color_f32);
                    }

                    let mat = state.selected_material;
                    let matched = MATERIAL_COLORS[mat as usize];
                    let matched_color = egui::Color32::from_rgb(
                        (matched[0] * 255.0) as u8,
                        (matched[1] * 255.0) as u8,
                        (matched[2] * 255.0) as u8,
                    );
                    ui.horizontal(|ui| {
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::hover());
                        ui.painter().rect_filled(rect, 2.0, matched_color);
                        ui.label(format!("Palette #{mat}"));
                    });
                    ui.separator();
                }
            }

            // --- Mask (before brush size, after color) ---
            let show_mask = !matches!(
                state.active_tool,
                EditorTool::Prefab | EditorTool::Select | EditorTool::ColorPick
            );
            if show_mask {
                ui.label("Mask");
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut state.mask.mode, MaskMode::Disabled, "Off");
                    ui.selectable_value(&mut state.mask.mode, MaskMode::Include, "Include");
                    ui.selectable_value(&mut state.mask.mode, MaskMode::Exclude, "Exclude");
                });
                if state.mask.mode == MaskMode::Disabled {
                    state.mask_picking = false;
                }
                if state.mask.mode != MaskMode::Disabled {
                    ui.horizontal(|ui| {
                        let pick_label = if state.mask_picking {
                            "Picking..."
                        } else {
                            "Pick from world"
                        };
                        if ui
                            .selectable_label(state.mask_picking, pick_label)
                            .clicked()
                        {
                            state.mask_picking = !state.mask_picking;
                        }
                        if ui.button("Clear all").clicked() {
                            state.mask.materials.clear();
                        }
                    });
                    // Display masked materials as colored swatches
                    if !state.mask.materials.is_empty() {
                        let mut to_remove = None;
                        ui.horizontal_wrapped(|ui| {
                            let mut sorted: Vec<_> = state.mask.materials.iter().copied().collect();
                            sorted.sort();
                            for mat in sorted {
                                let c = MATERIAL_COLORS[mat as usize];
                                let color = egui::Color32::from_rgb(
                                    (c[0] * 255.0) as u8,
                                    (c[1] * 255.0) as u8,
                                    (c[2] * 255.0) as u8,
                                );
                                let (rect, resp) = ui.allocate_exact_size(
                                    egui::vec2(18.0, 18.0),
                                    egui::Sense::click(),
                                );
                                ui.painter().rect_filled(rect, 2.0, color);
                                if resp
                                    .on_hover_text(format!("#{mat} (click to remove)"))
                                    .clicked()
                                {
                                    to_remove = Some(mat);
                                }
                            }
                        });
                        if let Some(mat) = to_remove {
                            state.mask.materials.remove(&mat);
                        }
                    }
                }
                ui.separator();
            }

            // Path-specific settings (after mask, before brush tools)
            if state.active_tool == EditorTool::Path {
                ui.label("Path Width");
                let mut pw = path_state.path_width;
                ui.add(egui::Slider::new(&mut pw, 1..=32));
                path_state.path_width = pw;

                ui.label("Mode");
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut path_state.mode, PathMode::Flatten, "Flatten");
                    ui.selectable_value(&mut path_state.mode, PathMode::Paint, "Paint");
                });

                ui.separator();
            }

            if is_brush_tool {
                // --- Brush size & shape ---
                ui.label("Brush Size");
                let mut radius = state.brush_radius;
                ui.add(egui::Slider::new(&mut radius, 1..=128));
                state.brush_radius = radius;

                ui.label("Brush Shape (B)");
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut state.brush_shape, BrushShape::Sphere, "Sphere");
                    ui.selectable_value(&mut state.brush_shape, BrushShape::Cube, "Cube");
                    ui.selectable_value(&mut state.brush_shape, BrushShape::Cylinder, "Cylinder");
                    ui.selectable_value(&mut state.brush_shape, BrushShape::Diamond, "Diamond");
                });

                if !matches!(
                    state.active_tool,
                    EditorTool::Water
                        | EditorTool::Foliage
                        | EditorTool::Raise
                        | EditorTool::Lower
                        | EditorTool::Smooth
                ) {
                    ui.label("Strength");
                    ui.add(
                        egui::Slider::new(&mut state.brush_strength, 0.01..=1.0).fixed_decimals(2),
                    );
                }
                ui.separator();

                // Sculpt-specific settings
                let is_sculpt = matches!(
                    state.active_tool,
                    EditorTool::Raise | EditorTool::Lower | EditorTool::Smooth
                );
                if is_sculpt {
                    if matches!(state.active_tool, EditorTool::Raise | EditorTool::Lower) {
                        ui.label("Step Size");
                        let mut step = state.sculpt_step;
                        ui.add(egui::Slider::new(&mut step, 1..=64));
                        state.sculpt_step = step;
                    }

                    if state.active_tool == EditorTool::Smooth {
                        ui.label("Kernel Size");
                        let mut kernel = state.smooth_kernel;
                        ui.add(egui::Slider::new(&mut kernel, 1..=5).suffix(" (NxN)"));
                        state.smooth_kernel = kernel;

                        ui.label("Iterations");
                        let mut iters = state.smooth_iterations;
                        ui.add(egui::Slider::new(&mut iters, 1..=10));
                        state.smooth_iterations = iters;
                    }

                    ui.separator();
                }

                // Place-specific: noise displacement
                if state.active_tool == EditorTool::Place {
                    ui.label("Noise Displacement");
                    let mut disp = state.noise_displacement;
                    ui.add(egui::Slider::new(&mut disp, 0..=32));
                    state.noise_displacement = disp;
                    ui.separator();
                }

                if matches!(state.active_tool, EditorTool::Place | EditorTool::Paint) {
                    ui.label("Color Jitter");
                    ui.add(egui::Slider::new(&mut state.color_jitter, 0.0..=1.0).fixed_decimals(2));
                    ui.separator();
                }

                if state.active_tool == EditorTool::Foliage {
                    ui.label("Foliage Action");
                    ui.horizontal(|ui| {
                        ui.selectable_value(
                            &mut state.foliage_action,
                            FoliageAction::Paint,
                            "Paint",
                        );
                        ui.selectable_value(
                            &mut state.foliage_action,
                            FoliageAction::Erase,
                            "Erase",
                        );
                    });
                    ui.label("Foliage Mode");
                    ui.horizontal(|ui| {
                        ui.selectable_value(
                            &mut state.foliage_mode,
                            FoliageMode::SingleLevel,
                            "Single Level",
                        );
                        ui.selectable_value(
                            &mut state.foliage_mode,
                            FoliageMode::Surface,
                            "Surface",
                        );
                    });
                    ui.label("Density");
                    ui.add(
                        egui::Slider::new(&mut state.foliage_density, 0.01..=1.0).fixed_decimals(2),
                    );
                    ui.separator();
                }

                if state.active_tool == EditorTool::Water {
                    ui.label("Water Action");
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut state.water_action, WaterAction::Place, "Place");
                        ui.selectable_value(&mut state.water_action, WaterAction::Remove, "Remove");
                    });
                    ui.separator();
                }
            }

            if state.active_tool == EditorTool::Prefab {
                ui.horizontal(|ui| {
                    ui.label("Search:");
                    ui.text_edit_singleline(&mut state.prefab_search);
                });

                if prefabs.entries.is_empty() {
                    ui.label("No .fbx prefabs found.");
                } else {
                    let search = state.prefab_search.to_lowercase();
                    egui::ScrollArea::vertical()
                        .max_height(220.0)
                        .auto_shrink(false)
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            for entry in &prefabs.entries {
                                if !search.is_empty()
                                    && !entry.name.to_lowercase().contains(&search)
                                {
                                    continue;
                                }
                                let selected = prefabs.selected_is(&entry.source_path);
                                if ui.selectable_label(selected, &entry.name).clicked() {
                                    selected_prefab = Some(entry.source_path.clone());
                                }
                            }
                        });
                }

                ui.separator();
            }
        });

    // Draw selection box wireframe overlay
    if state.active_tool == EditorTool::Select
        && sel.phase != SelectionPhase::None
        && let (Some(min), Some(max)) = (sel.aabb_min(), sel.aabb_max())
    {
        draw_selection_box(&egui_ctx.0, &camera, &window, min, max, sel.hovered_face);

        // Draw translation gizmo when selection is active
        if sel.phase == SelectionPhase::Selected || sel.phase == SelectionPhase::Moving {
            draw_gizmo(&egui_ctx.0, &camera, &window, min, max, &sel);
        }
    }

    // Draw path preview overlay.
    if state.active_tool == EditorTool::Path && !path_state.waypoints.is_empty() {
        super::draw_path_preview(&egui_ctx.0, &camera, &window, &path_state);
    }

    if let Some(source_path) = selected_prefab {
        prefabs.set_selected_source(source_path);
        state.active_tool = EditorTool::Prefab;
    }

    // Scroll wheel adjusts regenerate resolution and triggers regeneration
    // when the prefab tool is active and a prefab is selected.
    // Resolution updates immediately; regeneration is throttled until
    // scrolling settles (300ms without new scroll events).
    if state.active_tool == EditorTool::Prefab
        && prefabs.selected_entry().is_some()
        && !egui_ctx.0.is_using_pointer()
    {
        let scroll_y = egui_ctx.0.input(|i| {
            i.events
                .iter()
                .filter_map(|e| match e {
                    egui::Event::MouseWheel { delta, .. } => Some(delta.y),
                    _ => None,
                })
                .sum::<f32>()
        });
        if scroll_y != 0.0 {
            let step = if scroll_y > 0.0 { 4i32 } else { -4 };
            let new_res = (prefabs.regenerate_resolution as i32 + step).clamp(4, 256) as u32;
            prefabs.regenerate_resolution = new_res;
            state.prefab_scroll_last = Some(std::time::Instant::now());
        }
    }

    const SCROLL_THROTTLE: std::time::Duration = std::time::Duration::from_millis(50);
    if let Some(last) = state.prefab_scroll_last {
        if last.elapsed() >= SCROLL_THROTTLE {
            state.prefab_scroll_last = None;
            regenerate_selected = Some(prefabs.regenerate_resolution);
        }
    }

    if let Some(resolution) = regenerate_selected
        && let Some(entry) = prefabs.selected_entry_mut()
    {
        entry.queue_regenerate(resolution, "manual");
    }
}

const NEAR_CLIP: f32 = 0.1;

/// The 12 edges of a box, as pairs of corner indices.
const BOX_EDGES: [(usize, usize); 12] = [
    // bottom face
    (0, 1),
    (1, 3),
    (3, 2),
    (2, 0),
    // top face
    (4, 5),
    (5, 7),
    (7, 6),
    (6, 4),
    // vertical pillars
    (0, 4),
    (1, 5),
    (2, 6),
    (3, 7),
];

/// Corner indices for each face of the box (wound consistently for convex_polygon).
const FACE_CORNERS: [(Face, [usize; 4]); 6] = [
    (Face::YNeg, [0, 1, 3, 2]), // bottom  (y min)
    (Face::YPos, [4, 6, 7, 5]), // top     (y max)
    (Face::XNeg, [0, 2, 6, 4]), // left    (x min)
    (Face::XPos, [1, 5, 7, 3]), // right   (x max)
    (Face::ZNeg, [0, 4, 5, 1]), // front   (z min)
    (Face::ZPos, [2, 3, 7, 6]), // back    (z max)
];

fn draw_selection_box(
    ctx: &egui::Context,
    camera: &Camera,
    window: &GameWindow,
    min: glam::IVec3,
    max: glam::IVec3,
    hovered_face: Option<Face>,
) {
    let w = window.width as f32;
    let h = window.height as f32;
    if w < 1.0 || h < 1.0 {
        return;
    }

    // Build view-projection matrix matching the render pipeline
    let view = Mat4::look_to_rh(camera.position, camera.forward(), Vec3::Y);
    let proj = Mat4::perspective_infinite_rh(camera.fov_y, camera.aspect, NEAR_CLIP);
    let vp = proj * view;

    // Selection AABB corners in world space (expand by 0.5 so the box wraps around voxels)
    let fmin = Vec3::new(
        min.x as f32 - 0.01,
        min.y as f32 - 0.01,
        min.z as f32 - 0.01,
    );
    let fmax = Vec3::new(
        max.x as f32 + 1.01,
        max.y as f32 + 1.01,
        max.z as f32 + 1.01,
    );

    let corners = [
        Vec3::new(fmin.x, fmin.y, fmin.z), // 0
        Vec3::new(fmax.x, fmin.y, fmin.z), // 1
        Vec3::new(fmin.x, fmin.y, fmax.z), // 2
        Vec3::new(fmax.x, fmin.y, fmax.z), // 3
        Vec3::new(fmin.x, fmax.y, fmin.z), // 4
        Vec3::new(fmax.x, fmax.y, fmin.z), // 5
        Vec3::new(fmin.x, fmax.y, fmax.z), // 6
        Vec3::new(fmax.x, fmax.y, fmax.z), // 7
    ];

    // Project to clip space, then to screen pixels
    let projected: Vec<Option<egui::Pos2>> = corners
        .iter()
        .map(|&c| {
            let clip = vp * Vec4::new(c.x, c.y, c.z, 1.0);
            // Behind camera
            if clip.w <= 0.0 {
                return None;
            }
            let ndc = clip.truncate() / clip.w;
            let sx = (ndc.x * 0.5 + 0.5) * w;
            let sy = (1.0 - (ndc.y * 0.5 + 0.5)) * h;
            Some(egui::pos2(sx, sy))
        })
        .collect();

    let mut painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Background,
        egui::Id::new("selection_box"),
    ));
    painter.set_clip_rect(ctx.content_rect());

    // Draw face tints: all faces get a dark overlay, hovered face gets a bright highlight
    for &(face, ref idxs) in &FACE_CORNERS {
        let face_pts: Vec<egui::Pos2> = idxs.iter().filter_map(|&i| projected[i]).collect();
        if face_pts.len() == 4 {
            let is_hovered = hovered_face == Some(face);
            let (fill, stroke_style) = if is_hovered {
                (
                    egui::Color32::from_rgba_unmultiplied(0, 200, 255, 50),
                    egui::Stroke::new(2.5, egui::Color32::from_rgb(0, 230, 255)),
                )
            } else {
                (
                    egui::Color32::from_rgba_unmultiplied(0, 0, 0, 30),
                    egui::Stroke::NONE,
                )
            };
            painter.add(egui::Shape::convex_polygon(face_pts, fill, stroke_style));
        }
    }

    let stroke = egui::Stroke::new(1.5, egui::Color32::from_rgb(0, 200, 255));

    for &(a, b) in &BOX_EDGES {
        if let (Some(pa), Some(pb)) = (projected[a], projected[b]) {
            painter.line_segment([pa, pb], stroke);
        }
    }
}

/// Gizmo arrow length, matching the hit-test function in selection.rs.
fn gizmo_arrow_length(camera_pos: Vec3, center: Vec3) -> f32 {
    let dist = (camera_pos - center).length();
    (dist * 0.08).max(3.0)
}

/// Project a world-space point to screen-space egui coordinates.
/// Returns None if the point is behind or too close to the camera.
/// Clamps output to avoid extreme coordinates that cause rendering artifacts.
fn project_to_screen(vp: &Mat4, w: f32, h: f32, point: Vec3) -> Option<egui::Pos2> {
    let clip = *vp * Vec4::new(point.x, point.y, point.z, 1.0);
    if clip.w < 0.1 {
        return None;
    }
    let ndc = clip.truncate() / clip.w;
    let margin = 4000.0;
    let sx = ((ndc.x * 0.5 + 0.5) * w).clamp(-margin, w + margin);
    let sy = ((1.0 - (ndc.y * 0.5 + 0.5)) * h).clamp(-margin, h + margin);
    Some(egui::pos2(sx, sy))
}

fn draw_gizmo(
    ctx: &egui::Context,
    camera: &Camera,
    window: &GameWindow,
    min: glam::IVec3,
    max: glam::IVec3,
    sel: &SelectionState,
) {
    let w = window.width as f32;
    let h = window.height as f32;
    if w < 1.0 || h < 1.0 {
        return;
    }

    let view = Mat4::look_to_rh(camera.position, camera.forward(), Vec3::Y);
    let proj = Mat4::perspective_infinite_rh(camera.fov_y, camera.aspect, NEAR_CLIP);
    let vp = proj * view;

    let fmin = Vec3::new(min.x as f32, min.y as f32, min.z as f32);
    let fmax = Vec3::new(max.x as f32 + 1.0, max.y as f32 + 1.0, max.z as f32 + 1.0);
    let center = (fmin + fmax) * 0.5;
    let arrow_len = gizmo_arrow_length(camera.position, center);

    let mut painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("gizmo"),
    ));
    painter.set_clip_rect(ctx.content_rect());

    // Axis definitions: direction, color
    let axes: [(Vec3, egui::Color32); 3] = [
        (Vec3::X, egui::Color32::from_rgb(220, 50, 50)), // X = red
        (Vec3::Y, egui::Color32::from_rgb(80, 200, 50)), // Y = green
        (Vec3::Z, egui::Color32::from_rgb(50, 100, 220)), // Z = blue
    ];

    let hovered = sel.gizmo_hovered;
    let dragging = sel.gizmo_drag_axis;

    for (i, (dir, color)) in axes.iter().enumerate() {
        let is_active = dragging == Some(i) || (dragging.is_none() && hovered == Some(i));

        // Brighten color when hovered/active
        let line_color = if is_active {
            egui::Color32::from_rgb(
                (color.r() as u16 + 80).min(255) as u8,
                (color.g() as u16 + 80).min(255) as u8,
                (color.b() as u16 + 80).min(255) as u8,
            )
        } else {
            *color
        };

        let thickness = if is_active { 8.0 } else { 6.0 };
        let tip = center + *dir * arrow_len;

        let Some(p_center) = project_to_screen(&vp, w, h, center) else {
            continue;
        };
        let Some(p_tip) = project_to_screen(&vp, w, h, tip) else {
            continue;
        };

        // Draw shaft
        painter.line_segment([p_center, p_tip], egui::Stroke::new(thickness, line_color));

        // Draw arrowhead as a 2D triangle in screen space
        let shaft_dx = p_tip.x - p_center.x;
        let shaft_dy = p_tip.y - p_center.y;
        let shaft_len = (shaft_dx * shaft_dx + shaft_dy * shaft_dy).sqrt();
        if shaft_len > 1.0 {
            let fwd_x = shaft_dx / shaft_len;
            let fwd_y = shaft_dy / shaft_len;
            let perp_x = -fwd_y;
            let perp_y = fwd_x;

            let head_len = if is_active { 24.0 } else { 20.0 };
            let head_half_w = if is_active { 14.0 } else { 11.0 };

            let base_x = p_tip.x - fwd_x * head_len;
            let base_y = p_tip.y - fwd_y * head_len;

            let tri = vec![
                p_tip,
                egui::pos2(base_x + perp_x * head_half_w, base_y + perp_y * head_half_w),
                egui::pos2(base_x - perp_x * head_half_w, base_y - perp_y * head_half_w),
            ];
            painter.add(egui::Shape::convex_polygon(
                tri,
                line_color,
                egui::Stroke::NONE,
            ));
        }
    }

    // Draw center dot for free movement
    if let Some(p_center) = project_to_screen(&vp, w, h, center) {
        let center_active = dragging == Some(3) || (dragging.is_none() && hovered == Some(3));
        let dot_radius = if center_active { 8.0 } else { 6.0 };
        let dot_color = if center_active {
            egui::Color32::from_rgb(255, 255, 100)
        } else {
            egui::Color32::WHITE
        };
        painter.circle_filled(p_center, dot_radius, dot_color);
    }
}
