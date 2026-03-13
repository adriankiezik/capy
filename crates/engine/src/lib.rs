pub mod error;
pub use error::{EngineError, Result};

use std::sync::Arc;

use capy_render::Renderer;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

#[derive(Default)]
pub struct App {
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    error: Option<EngineError>,
}

impl App {
    fn fail(&mut self, event_loop: &ActiveEventLoop, error: impl Into<EngineError>) {
        if self.error.is_none() {
            self.error = Some(error.into());
        }
        event_loop.exit();
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            let attrs = Window::default_attributes().with_title("Capy");
            let window = match event_loop.create_window(attrs) {
                Ok(w) => Arc::new(w),
                Err(e) => return self.fail(event_loop, e),
            };
            let size = window.inner_size();
            let renderer = match Renderer::new(window.clone(), size.width, size.height) {
                Ok(r) => r,
                Err(e) => return self.fail(event_loop, e),
            };
            self.window = Some(window);
            self.renderer = Some(renderer);
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(renderer) = &mut self.renderer {
                    renderer.resize(size.width, size.height);
                }
            }
            WindowEvent::RedrawRequested => {
                let err = self.renderer.as_ref().and_then(|r| r.render().err());
                match err {
                    Some(capy_render::RenderError::Surface(
                        capy_render::SurfaceError::Lost | capy_render::SurfaceError::Outdated,
                    )) => {
                        if let Some(window) = &self.window {
                            let size = window.inner_size();
                            if let Some(r) = &mut self.renderer {
                                r.resize(size.width, size.height);
                            }
                        }
                    }
                    Some(capy_render::RenderError::Surface(
                        capy_render::SurfaceError::OutOfMemory,
                    )) => {
                        return self.fail(
                            event_loop,
                            capy_render::RenderError::Surface(
                                capy_render::SurfaceError::OutOfMemory,
                            ),
                        );
                    }
                    Some(e) => eprintln!("Render error: {e}"),
                    None => {}
                }
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            _ => {}
        }
    }
}

pub fn run() -> Result<()> {
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App::default();
    event_loop.run_app(&mut app)?;
    if let Some(error) = app.error {
        return Err(error);
    }
    Ok(())
}
