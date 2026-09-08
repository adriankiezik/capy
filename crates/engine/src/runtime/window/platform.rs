use crate::runtime::{WindowError, WindowSettings};
use std::sync::Arc;
use winit::{event_loop::ActiveEventLoop, window::Window as NativeWindow};

#[derive(Debug)]
pub struct Window {
    pub(in crate::runtime) native: Arc<NativeWindow>,
    pub(in crate::runtime) occluded: bool,
    pub(in crate::runtime) close_requested: bool,
    pub(in crate::runtime) exit_requested: bool,
    captured: bool,
    pub(in crate::runtime) capture_revision: u64,
}

impl Window {
    pub(in crate::runtime) fn create(
        event_loop: &ActiveEventLoop,
        settings: WindowSettings,
    ) -> Result<Self, WindowError> {
        Ok(Self {
            native: Arc::new(event_loop.create_window(settings)?),
            occluded: false,
            close_requested: false,
            exit_requested: false,
            captured: false,
            capture_revision: 0,
        })
    }

    pub(in crate::runtime) fn request_redraw(&self) {
        self.native.request_redraw();
    }

    pub fn capture_cursor(&mut self) -> Result<(), WindowError> {
        use winit::window::CursorGrabMode;

        if self.captured {
            return Ok(());
        }

        self.native
            .set_cursor_grab(CursorGrabMode::Locked)
            .or_else(|_| self.native.set_cursor_grab(CursorGrabMode::Confined))?;

        self.native.set_cursor_visible(false);

        self.captured = true;
        self.capture_revision = self.capture_revision.wrapping_add(1);

        Ok(())
    }

    pub fn release_cursor(&mut self) {
        if !self.captured {
            return;
        }

        let _ = self
            .native
            .set_cursor_grab(winit::window::CursorGrabMode::None);

        self.native.set_cursor_visible(true);

        self.captured = false;
        self.capture_revision = self.capture_revision.wrapping_add(1);
    }

    pub fn cursor_captured(&self) -> bool {
        self.captured
    }

    pub fn toggle_fullscreen(&self) {
        use winit::window::Fullscreen;

        self.native
            .set_fullscreen(if self.native.fullscreen().is_none() {
                Some(Fullscreen::Borderless(self.native.current_monitor()))
            } else {
                None
            });
    }

    pub fn native(&self) -> &NativeWindow {
        &self.native
    }

    pub fn occluded(&self) -> bool {
        self.occluded
    }

    pub fn close_requested(&self) -> bool {
        self.close_requested
    }

    pub fn close(&mut self) {
        self.exit_requested = true;
    }
}

impl AsRef<NativeWindow> for Window {
    fn as_ref(&self) -> &NativeWindow {
        &self.native
    }
}
