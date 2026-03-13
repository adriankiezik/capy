use raw_window_handle::{HasDisplayHandle, HasWindowHandle};

/// Trait alias that decouples the engine from any specific windowing library.
pub trait Window: HasWindowHandle + HasDisplayHandle + Send + Sync + 'static {}
impl<T: HasWindowHandle + HasDisplayHandle + Send + Sync + 'static> Window for T {}
