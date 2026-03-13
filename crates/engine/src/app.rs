use std::cell::RefCell;
use std::sync::Arc;

use capy_core::{BevyError, ErrorContext, GameWindow, ScheduleLabel, World};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowId};

use crate::EngineError;

thread_local! {
    static CAPTURED_ERROR: RefCell<Option<BevyError>> = const { RefCell::new(None) };
}

pub(crate) fn capture_error(error: BevyError, _ctx: ErrorContext) {
    CAPTURED_ERROR.with(|e| {
        *e.borrow_mut() = Some(error);
    });
}

fn take_captured_error() -> Option<BevyError> {
    CAPTURED_ERROR.with(|e| e.borrow_mut().take())
}

pub(crate) struct App {
    pub(crate) world: World,
    window: Option<Arc<Window>>,
    pub(crate) error: Option<EngineError>,
}

impl App {
    pub(crate) fn new(world: World) -> Self {
        Self {
            world,
            window: None,
            error: None,
        }
    }

    fn fail(&mut self, event_loop: &ActiveEventLoop, error: impl Into<EngineError>) {
        if self.error.is_none() {
            self.error = Some(error.into());
        }
        event_loop.exit();
    }

    fn run_schedule(&mut self, label: impl ScheduleLabel) -> std::result::Result<(), BevyError> {
        let _ = self.world.try_run_schedule(label);
        if let Some(err) = take_captured_error() {
            return Err(err);
        }
        Ok(())
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

            self.world.insert_resource(GameWindow {
                handle: window.clone(),
                width: size.width,
                height: size.height,
            });

            self.window = Some(window);

            if let Err(e) = self.run_schedule(capy_core::Startup) {
                self.fail(event_loop, e);
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(mut game_window) = self.world.get_resource_mut::<GameWindow>() {
                    game_window.width = size.width;
                    game_window.height = size.height;
                }
            }
            WindowEvent::RedrawRequested => {
                if let Err(e) = self.run_schedule(capy_core::Update) {
                    return self.fail(event_loop, e);
                }
                if let Err(e) = self.run_schedule(capy_core::Render) {
                    return self.fail(event_loop, e);
                }
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            _ => {}
        }
    }
}
