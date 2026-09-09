use crate::runtime::{Button, ButtonBinding, Input, Window, WindowError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowAction {
    ToggleFullscreen,
    CaptureCursor,
    ReleaseCursor,
    ToggleCursorCapture,
    ReleaseCursorOrClose,
    Close,
}

impl WindowAction {
    fn controls_cursor(self) -> bool {
        matches!(
            self,
            Self::CaptureCursor
                | Self::ReleaseCursor
                | Self::ToggleCursorCapture
                | Self::ReleaseCursorOrClose
        )
    }

    fn applicable(self, captured: bool) -> bool {
        match self {
            Self::CaptureCursor => !captured,
            Self::ReleaseCursor => captured,
            _ => true,
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct WindowControls {
    bindings: Vec<(ButtonBinding, WindowAction)>,
}

impl WindowControls {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn bind(mut self, button: impl Into<Button>, action: WindowAction) -> Self {
        self.bindings.push((ButtonBinding::new(button), action));

        self
    }

    pub(in crate::runtime) fn update(
        &self,
        input: &mut Input,
        window: &mut Window,
    ) -> Result<(), WindowError> {
        if !input.focused() {
            return Ok(());
        }

        let mut cursor_handled = false;

        for (binding, action) in &self.bindings {
            if !binding.state(input).just_pressed()
                || !action.applicable(window.cursor_captured())
                || (action.controls_cursor() && cursor_handled)
            {
                continue;
            }

            binding.consume(input);

            if action.controls_cursor() {
                cursor_handled = true;

                input.discard_mouse_motion();

                for (binding, action) in &self.bindings {
                    if action.controls_cursor() {
                        binding.consume(input);
                    }
                }
            }

            match action {
                WindowAction::ToggleFullscreen => window.toggle_fullscreen(),
                WindowAction::CaptureCursor => window.capture_cursor()?,
                WindowAction::ReleaseCursor => window.release_cursor(),
                WindowAction::ToggleCursorCapture => {
                    if window.cursor_captured() {
                        window.release_cursor();
                    } else {
                        window.capture_cursor()?;
                    }
                }
                WindowAction::ReleaseCursorOrClose => {
                    if window.cursor_captured() {
                        window.release_cursor();
                    } else {
                        window.close();
                    }
                }
                WindowAction::Close => window.close(),
            }

            if window.exit_requested {
                break;
            }
        }

        Ok(())
    }
}
