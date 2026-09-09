use engine::{
    Vec2,
    runtime::{Axis2Binding, InputMap, KeyCode, MouseButton, WindowAction, WindowControls},
};

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    Move,
    Jump,
    Sprint,
    Destroy,
}

pub fn window_controls() -> WindowControls {
    use KeyCode::*;

    WindowControls::new()
        .bind(F11, WindowAction::ToggleFullscreen)
        .bind(Escape, WindowAction::ReleaseCursorOrClose)
        .bind(Tab, WindowAction::ToggleCursorCapture)
        .bind(MouseButton::Left, WindowAction::CaptureCursor)
}

pub fn look(delta: Vec2) -> Vec2 {
    delta * Vec2::new(0.0025, -0.0025)
}

pub fn bindings() -> InputMap<Action> {
    use Action::*;

    use KeyCode::*;

    InputMap::new()
        .bind(
            Move,
            Axis2Binding::keys(KeyA, KeyD, KeyS, KeyW).normalized(),
        )
        .bind(Jump, Space)
        .bind(Sprint, ShiftLeft)
        .bind(Destroy, MouseButton::Left)
}
