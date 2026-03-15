use std::sync::Arc;

use bevy_ecs::schedule::ScheduleLabel;
use bevy_ecs::world::World;
use capy_core::{GameWindow, KeyboardInputMessage, MouseMotionMessage, Window as CoreWindow};
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, DeviceId, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::PhysicalKey;
use winit::window::{Window as WinitWindow, WindowId};

use crate::WindowError;
use crate::keys::convert_key;
use crate::resources::{OnAppResumed, OnBeginFrame, OnEndFrame, OnWindowEvent, WantsPointerInput};
use crate::window::WindowAdapter;

pub(crate) struct App {
    pub(crate) world: World,
    window: Option<Arc<WinitWindow>>,
    pub(crate) error: Option<WindowError>,
}

impl App {
    pub(crate) fn new(world: World) -> Self {
        Self {
            world,
            window: None,
            error: None,
        }
    }

    fn fail(&mut self, event_loop: &ActiveEventLoop, error: impl Into<WindowError>) {
        if self.error.is_none() {
            self.error = Some(error.into());
        }
        event_loop.exit();
    }

    fn run_schedule(
        &mut self,
        label: impl ScheduleLabel,
    ) -> std::result::Result<(), bevy_ecs::error::BevyError> {
        capy_engine::schedule_runner::run_schedule(&mut self.world, label)
    }

    fn dispatch_resumed(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(hooks) = self.world.remove_resource::<OnAppResumed>() {
            hooks.invoke(&mut self.world, event_loop);
            self.world.insert_resource(hooks);
        }
    }

    fn dispatch_window_event(&mut self, window: &WinitWindow, event: &WindowEvent) -> bool {
        if let Some(hooks) = self.world.remove_resource::<OnWindowEvent>() {
            let consumed = hooks.invoke(&mut self.world, window, event);
            self.world.insert_resource(hooks);
            consumed
        } else {
            false
        }
    }

    fn dispatch_begin_frame(&mut self, window: &WinitWindow) {
        if let Some(hooks) = self.world.remove_resource::<OnBeginFrame>() {
            hooks.invoke(&mut self.world, window);
            self.world.insert_resource(hooks);
        }
    }

    fn dispatch_end_frame(&mut self, window: &WinitWindow) {
        if let Some(hooks) = self.world.remove_resource::<OnEndFrame>() {
            hooks.invoke(&mut self.world, window);
            self.world.insert_resource(hooks);
        }
    }

    fn wants_pointer_input(&self) -> bool {
        self.world
            .get_resource::<WantsPointerInput>()
            .is_some_and(|hooks| hooks.invoke(&self.world))
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            let config = self
                .world
                .get_resource::<capy_core::WindowConfig>()
                .map(|c| (c.title.clone(), c.width, c.height))
                .unwrap_or_else(|| (String::from("Capy"), 1280, 720));

            let attrs = WinitWindow::default_attributes()
                .with_title(config.0)
                .with_inner_size(winit::dpi::LogicalSize::new(config.1, config.2));
            let window = match event_loop.create_window(attrs) {
                Ok(w) => Arc::new(w),
                Err(e) => return self.fail(event_loop, e),
            };
            let size = window.inner_size();
            let shared_window: Arc<dyn CoreWindow> = Arc::new(WindowAdapter::new(window.clone()));

            self.world.insert_resource(GameWindow {
                handle: shared_window,
                width: size.width,
                height: size.height,
            });
            self.dispatch_resumed(event_loop);

            self.window = Some(window);

            if let Err(e) = self.run_schedule(capy_core::PreStartup) {
                return self.fail(event_loop, e);
            }
            if let Err(e) = self.run_schedule(capy_core::Startup) {
                self.fail(event_loop, e);
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let event_consumed = if let Some(window) = self.window.clone() {
            self.dispatch_window_event(window.as_ref(), &event)
        } else {
            false
        };

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(mut game_window) = self.world.get_resource_mut::<GameWindow>() {
                    game_window.width = size.width;
                    game_window.height = size.height;
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(winit_key) = event.physical_key
                    && let Some(key) = convert_key(winit_key)
                    && (!event_consumed || !event.state.is_pressed())
                {
                    self.world.write_message(KeyboardInputMessage {
                        key,
                        pressed: event.state.is_pressed(),
                    });
                }
            }
            WindowEvent::RedrawRequested => {
                if let Some(window) = self.window.clone() {
                    self.dispatch_begin_frame(window.as_ref());
                }

                if let Err(e) = self.run_schedule(capy_core::Update) {
                    return self.fail(event_loop, e);
                }

                if let Some(window) = self.window.clone() {
                    self.dispatch_end_frame(window.as_ref());
                }

                if let Err(e) = self.run_schedule(capy_core::Render) {
                    return self.fail(event_loop, e);
                }
                if self.world.get_resource::<capy_core::AppExit>().is_some() {
                    event_loop.exit();
                    return;
                }
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            _ => {}
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: DeviceId,
        event: DeviceEvent,
    ) {
        if let DeviceEvent::MouseMotion { delta } = event {
            if self.wants_pointer_input() {
                return;
            }
            self.world.write_message(MouseMotionMessage {
                dx: delta.0,
                dy: delta.1,
            });
        }
    }
}
