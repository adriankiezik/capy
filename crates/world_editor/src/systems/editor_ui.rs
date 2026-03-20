use bevy_ecs::system::{Res, ResMut};
use capy_core::{Camera, GameWindow, MATERIAL_COLORS};
use capy_ui::EguiContext;
use glam::{Mat4, Vec3, Vec4};

use crate::resources::{
    BrushShape, Clipboard, EditHistory, EditorState, EditorTool, PrefabEntryStatus, PrefabLibrary,
    SaveResult, SaveState, SelectionPhase, SelectionState, VoxelHit,
};

#[allow(clippy::too_many_arguments)]
pub(crate) fn editor_ui(
    egui_ctx: Res<EguiContext>,
    mut state: ResMut<EditorState>,
    mut prefabs: ResMut<PrefabLibrary>,
    history: Res<EditHistory>,
    voxel_hit: Option<Res<VoxelHit>>,
    mut sel: ResMut<SelectionState>,
    clipboard: Res<Clipboard>,
    camera: Res<Camera>,
    window: Res<GameWindow>,
    mut save_state: ResMut<SaveState>,
) {
    let mut selected_prefab = None;
    let mut regenerate_selected = None;

    // Snapshot only lightweight display fields from the selected entry.
    // Avoids cloning the multi-MB VoxelPrefabAsset every frame.
    let selected_info = prefabs.selected_entry().map(|entry| SelectedEntryInfo {
        name: entry.name.clone(),
        source_path: entry.source_path.clone(),
        cache_path: entry.cache_path.clone(),
        status: entry.status.clone(),
        metadata: entry.metadata.clone(),
        has_prefab: entry.prefab.is_some(),
        display_resolution: entry.display_resolution(),
    });

    egui::Window::new("World Editor")
        .default_open(true)
        .resizable(false)
        .show(&egui_ctx.0, |ui| {
            if ui.button("Save World (Ctrl+S)").clicked() {
                save_state.requested = true;
            }
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
            ui.separator();

            ui.label("Tool");
            ui.horizontal(|ui| {
                ui.selectable_value(&mut state.active_tool, EditorTool::Place, "Place (1)");
                ui.selectable_value(&mut state.active_tool, EditorTool::Remove, "Remove (2)");
                ui.selectable_value(&mut state.active_tool, EditorTool::Paint, "Paint (3)");
            });
            ui.horizontal(|ui| {
                ui.selectable_value(&mut state.active_tool, EditorTool::Raise, "Raise (4)");
                ui.selectable_value(&mut state.active_tool, EditorTool::Lower, "Lower (5)");
                ui.selectable_value(&mut state.active_tool, EditorTool::Flatten, "Flat (6)");
                ui.selectable_value(&mut state.active_tool, EditorTool::Smooth, "Smooth (7)");
            });
            ui.horizontal(|ui| {
                ui.selectable_value(&mut state.active_tool, EditorTool::Prefab, "Prefab (8)");
                ui.selectable_value(&mut state.active_tool, EditorTool::Select, "Select (9)");
            });
            ui.separator();

            if state.active_tool == EditorTool::Select {
                if let Some(dims) = sel.dimensions() {
                    ui.label(format!("{} x {} x {} voxels", dims.x, dims.y, dims.z));
                }
                ui.checkbox(&mut sel.include_air, "Include air");

                let has_selection = sel.phase == SelectionPhase::Selected;
                ui.horizontal(|ui| {
                    ui.add_enabled(has_selection, egui::Button::new("Copy"))
                        .clicked()
                        .then(|| sel.phase = SelectionPhase::Selected); // handled by selection_system via Ctrl+C
                    ui.add_enabled(has_selection, egui::Button::new("Cut"));
                    ui.add_enabled(!clipboard.is_empty(), egui::Button::new("Paste"));
                    ui.add_enabled(has_selection, egui::Button::new("Delete"));
                });
                match sel.phase {
                    SelectionPhase::None => ui.small("Drag to select a region."),
                    SelectionPhase::Dragging => ui.small("Release to finalize selection."),
                    SelectionPhase::Selected => {
                        ui.small("Ctrl+C/X/V, Del. Click face to resize. G move. R rotate.")
                    }
                    SelectionPhase::Resizing => ui.small("Release to finalize resize."),
                    SelectionPhase::Moving => ui.small("Click to confirm. Escape to cancel."),
                    SelectionPhase::Pasting => {
                        ui.small("Click to stamp. R rotate. Escape to cancel.")
                    }
                };
                ui.separator();
            }

            if state.active_tool != EditorTool::Prefab && state.active_tool != EditorTool::Select {
                ui.label("Brush Size");
                let mut radius = state.brush_radius;
                ui.add(egui::Slider::new(&mut radius, 1..=128));
                state.brush_radius = radius;

                ui.label("Brush Shape (B)");
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut state.brush_shape, BrushShape::Sphere, "Sphere");
                    ui.selectable_value(&mut state.brush_shape, BrushShape::Cube, "Cube");
                });
                ui.separator();

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

            ui.collapsing("Prefabs", |ui| {
                ui.small(prefabs.source_dir.display().to_string());
                ui.label(format!(
                    "Ready: {} / {}",
                    prefabs.ready_count(),
                    prefabs.entries.len()
                ));
                ui.label(format!(
                    "Startup resolution: {}",
                    prefabs.default_resolution
                ));
                ui.separator();

                if prefabs.entries.is_empty() {
                    ui.label("No .fbx prefabs found.");
                } else {
                    egui::ScrollArea::vertical()
                        .max_height(220.0)
                        .show(ui, |ui| {
                            for entry in &prefabs.entries {
                                let selected = prefabs.selected_is(&entry.source_path);
                                if ui.selectable_label(selected, &entry.name).clicked() {
                                    selected_prefab = Some(entry.source_path.clone());
                                }
                                ui.small(entry.status_label());
                                ui.small(entry.source_path.display().to_string());
                                ui.add_space(6.0);
                            }
                        });
                }

                if let Some(info) = selected_info.as_ref() {
                    ui.separator();
                    ui.heading(&info.name);
                    ui.small(info.source_path.display().to_string());
                    ui.small(info.cache_path.display().to_string());
                    ui.label(info.status_label());

                    let active_job = matches!(
                        info.status,
                        PrefabEntryStatus::LoadingCache | PrefabEntryStatus::Voxelizing { .. }
                    );

                    ui.label("Regenerate Cache");
                    ui.add(egui::Slider::new(
                        &mut prefabs.regenerate_resolution,
                        4..=256,
                    ));
                    if ui
                        .add_enabled(!active_job, egui::Button::new("Regenerate .voxel"))
                        .clicked()
                    {
                        regenerate_selected = Some(prefabs.regenerate_resolution);
                    }

                    if let Some(metadata) = info.metadata.as_ref() {
                        ui.separator();
                        ui.label(format!(
                            "{}x{}x{} voxels",
                            metadata.size[0], metadata.size[1], metadata.size[2]
                        ));
                        ui.label(format!("Filled: {}", metadata.filled_voxel_count));
                        ui.label(format!("Resolution: {}", info.display_resolution));
                    }

                    if info.has_prefab {
                        ui.label("Select Prefab (8), then left click a surface to place it.");
                    } else {
                        ui.separator();
                        match info.status {
                            PrefabEntryStatus::LoadingCache => {
                                ui.label("Loading cached prefab data...");
                            }
                            PrefabEntryStatus::Ready => {
                                ui.label("Cached prefab will load when selected.");
                            }
                            _ => {
                                ui.label("Prefab is not ready yet.");
                            }
                        }
                    }
                } else if !prefabs.entries.is_empty() {
                    ui.separator();
                    ui.label("Select a prefab to load its cached voxel data.");
                }
            });
            ui.separator();

            ui.label(format!(
                "Undo: {} | Redo: {}",
                history.undo_stack.len(),
                history.redo_stack.len()
            ));
            ui.label("Ctrl+Z / Ctrl+Y");
            ui.separator();

            if let Some(hit) = voxel_hit {
                ui.label("Voxel Pick");
                if hit.hit {
                    ui.label(format!(
                        "Position: ({:.1}, {:.1}, {:.1})",
                        hit.position.x, hit.position.y, hit.position.z
                    ));
                    ui.label(format!(
                        "Normal: ({:.0}, {:.0}, {:.0})",
                        hit.normal.x, hit.normal.y, hit.normal.z
                    ));
                    ui.label(format!("Material: {}", hit.material));

                    let voxel = (hit.position - hit.normal * 0.5).floor();
                    ui.label(format!(
                        "Voxel: ({}, {}, {})",
                        voxel.x as i32, voxel.y as i32, voxel.z as i32
                    ));
                } else {
                    ui.label("No hit");
                }
            }
        });

    // Draw selection box wireframe overlay
    if state.active_tool == EditorTool::Select
        && sel.phase != SelectionPhase::None
        && let (Some(min), Some(max)) = (sel.aabb_min(), sel.aabb_max())
    {
        draw_selection_box(&egui_ctx.0, &camera, &window, min, max);
    }

    if let Some(source_path) = selected_prefab {
        prefabs.set_selected_source(source_path);
        state.active_tool = EditorTool::Prefab;
    }

    if let Some(resolution) = regenerate_selected
        && let Some(entry) = prefabs.selected_entry_mut()
    {
        entry.queue_regenerate(resolution, "manual");
    }
}

/// Lightweight snapshot of `PrefabEntry` for UI display, avoiding the
/// multi-MB `VoxelPrefabAsset` clone.
struct SelectedEntryInfo {
    name: String,
    source_path: std::path::PathBuf,
    cache_path: std::path::PathBuf,
    status: PrefabEntryStatus,
    metadata: Option<capy_assets::VoxelPrefabMetadata>,
    has_prefab: bool,
    display_resolution: u32,
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

fn draw_selection_box(
    ctx: &egui::Context,
    camera: &Camera,
    window: &GameWindow,
    min: glam::IVec3,
    max: glam::IVec3,
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

    let stroke = egui::Stroke::new(1.5, egui::Color32::from_rgb(0, 200, 255));

    for &(a, b) in &BOX_EDGES {
        if let (Some(pa), Some(pb)) = (projected[a], projected[b]) {
            painter.line_segment([pa, pb], stroke);
        }
    }
}

impl SelectedEntryInfo {
    fn status_label(&self) -> String {
        match &self.status {
            PrefabEntryStatus::QueuedLoad => String::from("Queued"),
            PrefabEntryStatus::QueuedRegenerate { resolution, reason } => {
                format!("Queued {resolution} ({reason})")
            }
            PrefabEntryStatus::LoadingCache => String::from("Loading cache"),
            PrefabEntryStatus::Voxelizing { resolution, reason } => {
                format!("Voxelizing {resolution} ({reason})")
            }
            PrefabEntryStatus::Ready => format!("Ready {}", self.display_resolution),
            PrefabEntryStatus::Error(message) => format!("Error: {message}"),
        }
    }
}
