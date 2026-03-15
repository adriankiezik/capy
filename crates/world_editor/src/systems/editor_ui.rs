use bevy_ecs::system::Res;
use capy_ui::EguiContext;

use crate::resources::VoxelHit;

pub(crate) fn editor_ui(egui_ctx: Res<EguiContext>, voxel_hit: Option<Res<VoxelHit>>) {
    egui::SidePanel::left("editor_panel")
        .default_width(220.0)
        .show(&egui_ctx.0, |ui| {
            ui.heading("World Editor");
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
}
