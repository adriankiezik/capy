use glam::Vec2;
use std::{collections::HashSet, time::Duration};
use winit::{
    event::MouseButton,
    keyboard::{KeyCode, PhysicalKey},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Button {
    Key(PhysicalKey),
    Mouse(MouseButton),
}

impl From<KeyCode> for Button {
    fn from(key: KeyCode) -> Self {
        Self::Key(key.into())
    }
}

impl From<PhysicalKey> for Button {
    fn from(key: PhysicalKey) -> Self {
        Self::Key(key)
    }
}

impl From<MouseButton> for Button {
    fn from(button: MouseButton) -> Self {
        Self::Mouse(button)
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ButtonState {
    pub(super) pressed: bool,
    pub(super) just_pressed: bool,
    pub(super) just_released: bool,
}

impl ButtonState {
    pub fn pressed(self) -> bool {
        self.pressed
    }

    pub fn just_pressed(self) -> bool {
        self.just_pressed
    }

    pub fn just_released(self) -> bool {
        self.just_released
    }

    pub fn active(self) -> bool {
        self.pressed || self.just_pressed
    }
}

#[derive(Debug, Clone)]
pub struct ButtonBinding {
    pub(super) buttons: Vec<Button>,
}

impl ButtonBinding {
    pub fn new(button: impl Into<Button>) -> Self {
        Self {
            buttons: vec![button.into()],
        }
    }

    pub fn or(mut self, button: impl Into<Button>) -> Self {
        let button = button.into();

        if !self.buttons.contains(&button) {
            self.buttons.push(button);
        }

        self
    }

    pub fn state(&self, input: &Input) -> ButtonState {
        input.button_state(&self.buttons)
    }

    pub(super) fn consume(&self, input: &mut Input) {
        for &button in &self.buttons {
            input.consume_until_release(button);
        }
    }
}

#[derive(Debug, Clone)]
pub struct AxisBinding {
    negative: ButtonBinding,
    positive: ButtonBinding,
}

impl AxisBinding {
    pub fn new(negative: ButtonBinding, positive: ButtonBinding) -> Self {
        Self { negative, positive }
    }

    pub fn keys(negative: KeyCode, positive: KeyCode) -> Self {
        Self::new(ButtonBinding::new(negative), ButtonBinding::new(positive))
    }

    pub fn value(&self, input: &Input) -> f32 {
        f32::from(self.positive.state(input).pressed())
            - f32::from(self.negative.state(input).pressed())
    }

    pub(super) fn buttons(&self) -> impl Iterator<Item = Button> + '_ {
        self.negative
            .buttons
            .iter()
            .chain(&self.positive.buttons)
            .copied()
    }
}

#[derive(Debug, Clone)]
enum Axis2Source {
    Buttons { x: AxisBinding, y: AxisBinding },
    MouseMotion,
}

#[derive(Debug, Clone)]
pub struct Axis2Binding {
    source: Axis2Source,
    scale: Vec2,
    per_second: bool,
    normalized: bool,
    captured_only: bool,
}

impl Axis2Binding {
    pub fn new(x: AxisBinding, y: AxisBinding) -> Self {
        Self {
            source: Axis2Source::Buttons { x, y },
            scale: Vec2::ONE,
            per_second: false,
            normalized: false,
            captured_only: false,
        }
    }

    pub fn keys(left: KeyCode, right: KeyCode, down: KeyCode, up: KeyCode) -> Self {
        Self::new(AxisBinding::keys(left, right), AxisBinding::keys(down, up))
    }

    pub fn mouse_motion() -> Self {
        Self {
            source: Axis2Source::MouseMotion,
            scale: Vec2::ONE,
            per_second: false,
            normalized: false,
            captured_only: false,
        }
    }

    pub fn scale(mut self, scale: Vec2) -> Self {
        self.scale = scale;

        self
    }

    pub fn rate(mut self, units_per_second: f32) -> Self {
        self.scale = Vec2::splat(units_per_second);
        self.per_second = true;

        self
    }

    pub fn normalized(mut self) -> Self {
        self.normalized = true;

        self
    }

    pub fn when_cursor_captured(mut self) -> Self {
        self.captured_only = true;

        self
    }

    pub fn value(&self, input: &Input, delta: Duration) -> Vec2 {
        if !self.enabled(input) {
            return Vec2::ZERO;
        }

        let mut value = match &self.source {
            Axis2Source::Buttons { x, y } => Vec2::new(x.value(input), y.value(input)),
            Axis2Source::MouseMotion if self.captured_only => input.captured_mouse_delta,
            Axis2Source::MouseMotion => input.mouse_delta,
        };

        if self.normalized {
            value = value.clamp_length_max(1.0);
        }

        value *= self.scale;

        if self.per_second {
            value *= delta.as_secs_f32();
        }

        value
    }

    pub(super) fn buttons(&self) -> Vec<Button> {
        match &self.source {
            Axis2Source::Buttons { x, y } => x.buttons().chain(y.buttons()).collect(),
            Axis2Source::MouseMotion => Vec::new(),
        }
    }

    pub(super) fn enabled(&self, input: &Input) -> bool {
        !self.captured_only || input.cursor_captured
    }

    pub(super) fn uses_mouse_motion(&self) -> bool {
        matches!(self.source, Axis2Source::MouseMotion)
    }
}

#[derive(Debug, Default, Clone)]
pub struct Input {
    down: HashSet<Button>,
    pub(super) initial_down: HashSet<Button>,
    changes: Vec<(Button, bool)>,
    consumed: HashSet<Button>,
    mouse_delta: Vec2,
    captured_mouse_delta: Vec2,
    cursor_captured: bool,
    capture_revision: u64,
    focused: bool,
}

impl Input {
    pub fn button(&self, button: impl Into<Button>) -> ButtonState {
        self.button_state(&[button.into()])
    }

    pub(super) fn button_state(&self, buttons: &[Button]) -> ButtonState {
        let mut held = buttons
            .iter()
            .filter(|button| self.initial_down.contains(button) && !self.consumed.contains(button))
            .count();

        let mut state = ButtonState::default();

        for &(button, down) in &self.changes {
            if !buttons.contains(&button) || self.consumed.contains(&button) {
                continue;
            }

            let was_down = held > 0;

            if down {
                held += 1;
            } else {
                held -= 1;
            }

            state.just_pressed |= !was_down && held > 0;
            state.just_released |= was_down && held == 0;
        }

        state.pressed = held > 0;

        state
    }

    pub(super) fn hide(&mut self, button: Button) {
        self.initial_down.remove(&button);

        self.changes.retain(|&(changed, _)| changed != button);
    }

    pub(super) fn suppress_held(&mut self, button: Button) -> bool {
        self.initial_down.remove(&button);

        let mut blocked = true;

        self.changes.retain(|&(changed, down)| {
            if changed != button || !blocked {
                return true;
            }

            if !down {
                blocked = false;
            }

            false
        });

        blocked && self.down.contains(&button)
    }

    pub(super) fn consume_until_release(&mut self, button: Button) {
        self.hide(button);

        if self.down.contains(&button) {
            self.consumed.insert(button);
        }
    }

    pub fn mouse_delta(&self) -> Vec2 {
        self.mouse_delta
    }

    pub(super) fn discard_mouse_motion(&mut self) {
        self.mouse_delta = Vec2::ZERO;
        self.captured_mouse_delta = Vec2::ZERO;
    }

    pub fn focused(&self) -> bool {
        self.focused
    }

    pub(super) fn sync_cursor(&mut self, captured: bool, revision: u64) {
        if self.capture_revision != revision {
            self.discard_mouse_motion();

            self.capture_revision = revision;
        }

        self.cursor_captured = captured;
    }

    pub(super) fn focus(&mut self, focused: bool) {
        self.focused = focused;

        if !focused {
            self.release_all();
        }
    }

    pub(super) fn motion(&mut self, delta: (f64, f64)) {
        if self.focused {
            let delta = Vec2::new(delta.0 as f32, delta.1 as f32);

            self.mouse_delta += delta;

            if self.cursor_captured {
                self.captured_mouse_delta += delta;
            }
        }
    }

    pub(super) fn change(&mut self, button: impl Into<Button>, down: bool) {
        let button = button.into();

        if down {
            if self.focused && self.down.insert(button) && !self.consumed.contains(&button) {
                self.changes.push((button, true));
            }
        } else if self.down.remove(&button) && !self.consumed.remove(&button) {
            self.changes.push((button, false));
        }
    }

    pub(super) fn release_all(&mut self) {
        self.changes.clear();

        self.changes.extend(
            self.initial_down
                .iter()
                .copied()
                .map(|button| (button, false)),
        );

        self.down.clear();

        self.consumed.clear();

        self.discard_mouse_motion();
    }

    pub(super) fn finish_update(&mut self) {
        self.initial_down.clear();

        self.initial_down
            .extend(self.down.difference(&self.consumed).copied());

        self.changes.clear();

        self.discard_mouse_motion();
    }
}
