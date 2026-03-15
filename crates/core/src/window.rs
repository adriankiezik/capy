use raw_window_handle::{HasDisplayHandle, HasWindowHandle};

pub trait Window: HasWindowHandle + HasDisplayHandle + Send + Sync + 'static {
    fn set_cursor_visible(&self, visible: bool);
    fn confine_or_lock_cursor(&self);
    fn release_cursor(&self);
    fn set_cursor_position(&self, x: f64, y: f64);
}
