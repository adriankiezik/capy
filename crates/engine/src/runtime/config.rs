use crate::graphics::GraphicsSettings;
use crate::runtime::WindowSettings;
use std::time::Duration;
use winit::keyboard::PhysicalKey;

#[derive(Debug, Default)]
pub struct Settings {
    pub(super) window: WindowSettings,
    pub(super) graphics: GraphicsSettings,
    pub(super) runtime: RuntimeSettings,
}

impl Settings {
    #[must_use]
    pub fn with_window(mut self, settings: WindowSettings) -> Self {
        self.window = settings;

        self
    }

    #[must_use]
    pub fn with_graphics(mut self, settings: GraphicsSettings) -> Self {
        self.graphics = settings;

        self
    }

    #[must_use]
    pub fn with_runtime(mut self, settings: RuntimeSettings) -> Self {
        self.runtime = settings;

        self
    }
}

#[derive(Clone, Debug)]
pub struct RuntimeSettings {
    pub(super) exit_key: Option<PhysicalKey>,
    pub(super) retry_delay: Duration,
    pub(super) continuous_redraw: bool,
    pub(super) close_on_request: bool,
}

impl Default for RuntimeSettings {
    fn default() -> Self {
        Self {
            exit_key: None,
            retry_delay: Duration::from_millis(100),
            continuous_redraw: true,
            close_on_request: true,
        }
    }
}

impl RuntimeSettings {
    #[must_use]
    pub fn with_exit_key(mut self, key: impl Into<PhysicalKey>) -> Self {
        self.exit_key = Some(key.into());

        self
    }

    #[must_use]
    pub fn with_retry_delay(mut self, delay: Duration) -> Self {
        self.retry_delay = delay;

        self
    }

    #[must_use]
    pub fn with_continuous_redraw(mut self, enabled: bool) -> Self {
        self.continuous_redraw = enabled;

        self
    }

    #[must_use]
    pub fn with_close_on_request(mut self, enabled: bool) -> Self {
        self.close_on_request = enabled;

        self
    }
}
