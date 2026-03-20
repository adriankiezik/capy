use bevy_ecs::system::{Res, ResMut};
use capy_core::RawInput;
use capy_ui::EguiContext;

use crate::resources::InputEdge;

pub(crate) fn input_edges(
    input: Res<RawInput>,
    egui_ctx: Res<EguiContext>,
    mut edge: ResMut<InputEdge>,
) {
    edge.update(&input);

    // Clear mouse edges when pointer is over egui UI to prevent click-through.
    // At this point in the frame editor_ui hasn't run yet, so the egui context
    // reflects the previous frame's fully-built UI (panels, popups, color
    // pickers, etc.) — exactly the state we want to gate on.
    if egui_ctx.0.egui_wants_pointer_input() {
        edge.mouse_just_pressed.clear();
        edge.mouse_just_released.clear();
    }
}
