use std::sync::Arc;

use capy_core::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, Window as CoreWindow,
    WindowHandle,
};
use winit::window::{CursorGrabMode, Window as WinitWindow};

pub(crate) struct WindowAdapter {
    inner: Arc<WinitWindow>,
}

impl WindowAdapter {
    pub(crate) fn new(inner: Arc<WinitWindow>) -> Self {
        Self { inner }
    }
}

impl HasWindowHandle for WindowAdapter {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        self.inner.window_handle()
    }
}

impl HasDisplayHandle for WindowAdapter {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        self.inner.display_handle()
    }
}

impl CoreWindow for WindowAdapter {
    fn set_cursor_visible(&self, visible: bool) {
        self.inner.set_cursor_visible(visible);
    }

    fn confine_or_lock_cursor(&self) {
        let _ = self
            .inner
            .set_cursor_grab(CursorGrabMode::Confined)
            .or_else(|_| self.inner.set_cursor_grab(CursorGrabMode::Locked));
    }

    fn release_cursor(&self) {
        let _ = self.inner.set_cursor_grab(CursorGrabMode::None);
    }

    fn set_cursor_position(&self, x: f64, y: f64) {
        let _ = self
            .inner
            .set_cursor_position(winit::dpi::PhysicalPosition::new(x, y));
    }
}
