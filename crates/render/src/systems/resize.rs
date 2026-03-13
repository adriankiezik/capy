use capy_core::{GameWindow, NonSendMut, Res};

use crate::resources::Renderer;

pub fn resize_system(mut renderer: NonSendMut<Renderer>, window: Res<GameWindow>) {
    if renderer.config.width != window.width || renderer.config.height != window.height {
        renderer.resize(window.width, window.height);
    }
}
