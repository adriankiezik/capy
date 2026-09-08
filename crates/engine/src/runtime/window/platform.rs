use crate::runtime::{WindowError, WindowSettings};
use std::sync::Arc;
use winit::{event_loop::ActiveEventLoop, window::Window as NativeWindow};

#[derive(Debug)]
pub struct Window {
    pub(in crate::runtime) native: Arc<NativeWindow>,
    pub(in crate::runtime) occluded: bool,
    pub(in crate::runtime) close_requested: bool,
    pub(in crate::runtime) exit_requested: bool,
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
        })
    }

    pub(in crate::runtime) fn request_redraw(&self) {
        self.native.request_redraw();
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
