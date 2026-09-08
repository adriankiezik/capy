use crate::graphics::{Frame, FrameOutcome, Graphics};
use crate::runtime::{ApplicationResult, Input, Result, RuntimeError, Settings, Window};
use std::time::{Duration, Instant};
use winit::{
    application::ApplicationHandler,
    event::{ElementState, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::WindowId,
};

#[derive(Debug)]
pub struct Context<'a> {
    window: &'a mut Window,
    graphics: &'a mut Graphics,
}

impl Context<'_> {
    pub fn window(&mut self) -> &mut Window {
        self.window
    }

    pub fn graphics(&mut self) -> &mut Graphics {
        self.graphics
    }
}

#[derive(Debug)]
pub struct Update<'a> {
    elapsed: Duration,
    input: &'a Input,
    context: Context<'a>,
}

impl<'a> Update<'a> {
    pub fn elapsed(&self) -> Duration {
        self.elapsed
    }

    pub fn input(&self) -> &Input {
        self.input
    }

    pub fn context(&mut self) -> &mut Context<'a> {
        &mut self.context
    }
}

pub trait Application {
    fn update(&mut self, update: Update<'_>) -> ApplicationResult<()>;

    fn draw(&self, frame: &mut Frame<'_>) -> ApplicationResult<()>;
}

pub fn run<A: Application>(
    settings: Settings,
    create: impl FnOnce(&mut Context<'_>) -> ApplicationResult<A>,
) -> Result<()> {
    if settings.runtime.retry_delay.is_zero()
        || Instant::now()
            .checked_add(settings.runtime.retry_delay)
            .is_none()
    {
        return Err(RuntimeError::InvalidRetryDelay);
    }

    let mut runtime = Runtime {
        settings,
        lifecycle: Lifecycle::Pending(create),
        input: Input::default(),
        next_redraw: None,
        last_update: None,
        error: None,
    };

    EventLoop::new()?.run_app(&mut runtime)?;

    if let Some(error) = runtime.error {
        return Err(error);
    }

    Ok(())
}

struct Session<A> {
    application: A,
    graphics: Graphics,
    window: Window,
}

impl<A: Application> Session<A> {
    fn new(
        event_loop: &ActiveEventLoop,
        settings: &mut Settings,
        create: impl FnOnce(&mut Context<'_>) -> ApplicationResult<A>,
    ) -> Result<Self> {
        let mut window = Window::create(event_loop, settings.window.clone())?;

        let size = window.native.inner_size();

        let mut graphics = pollster::block_on(Graphics::new(
            window.native.clone(),
            size.width,
            size.height,
            std::mem::take(&mut settings.graphics),
        ))?;

        let application = create(&mut Context {
            window: &mut window,
            graphics: &mut graphics,
        })
        .map_err(RuntimeError::Initialize)?;

        Ok(Self {
            application,
            graphics,
            window,
        })
    }
}

enum Lifecycle<A, F> {
    Pending(F),
    Active(Session<A>),
    Suspended(Session<A>),
    Stopped,
}

struct Runtime<A, F> {
    settings: Settings,
    lifecycle: Lifecycle<A, F>,
    input: Input,
    next_redraw: Option<Instant>,
    last_update: Option<Instant>,
    error: Option<RuntimeError>,
}

impl<A, F> Runtime<A, F> {
    fn fail(&mut self, event_loop: &ActiveEventLoop, error: RuntimeError) {
        self.error = Some(error);
        self.lifecycle = Lifecycle::Stopped;

        event_loop.exit();
    }
}

impl<A: Application, F: FnOnce(&mut Context<'_>) -> ApplicationResult<A>> ApplicationHandler
    for Runtime<A, F>
{
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let lifecycle = std::mem::replace(&mut self.lifecycle, Lifecycle::Stopped);

        let result = match lifecycle {
            Lifecycle::Pending(create) => Session::new(event_loop, &mut self.settings, create),
            Lifecycle::Suspended(mut session) => {
                let size = session.window.native.inner_size();

                session
                    .graphics
                    .attach(session.window.native.clone(), size.width, size.height)
                    .map(|()| session)
                    .map_err(RuntimeError::from)
            }
            lifecycle => {
                self.lifecycle = lifecycle;

                return;
            }
        };

        match result {
            Ok(mut session) => {
                session.window.occluded = false;

                session.window.request_redraw();

                self.lifecycle = Lifecycle::Active(session);
                self.last_update = None;
                self.next_redraw = None;
            }
            Err(error) => self.fail(event_loop, error),
        }
    }

    fn suspended(&mut self, _: &ActiveEventLoop) {
        let lifecycle = std::mem::replace(&mut self.lifecycle, Lifecycle::Stopped);

        self.lifecycle = match lifecycle {
            Lifecycle::Active(mut session) => {
                session.graphics.detach();

                Lifecycle::Suspended(session)
            }
            lifecycle => lifecycle,
        };
        self.input = Input::default();
        self.last_update = None;
        self.next_redraw = None;
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let (session, active) = match &mut self.lifecycle {
            Lifecycle::Active(session) => (session, true),
            Lifecycle::Suspended(session) => (session, false),
            Lifecycle::Pending(_) | Lifecycle::Stopped => return,
        };

        let window = &mut session.window;

        if window.native.id() != id {
            return;
        }

        match event {
            WindowEvent::CloseRequested => {
                window.close_requested = true;

                if self.settings.runtime.close_on_request {
                    event_loop.exit();
                } else {
                    window.request_redraw();
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let key = event.physical_key;

                let down = event.state == ElementState::Pressed;

                self.input.change(key, down);

                if self.settings.runtime.exit_key == Some(key) && down {
                    event_loop.exit();
                }

                window.request_redraw();
            }
            WindowEvent::Focused(false) => self.input.release_all(),
            WindowEvent::Resized(size) => {
                let result = session.graphics.resize(size.width, size.height);

                self.next_redraw = None;

                if size.width == 0 || size.height == 0 {
                    self.last_update = None;
                }

                window.request_redraw();

                if let Err(error) = result {
                    self.fail(event_loop, error.into());
                }
            }
            WindowEvent::Occluded(occluded) => {
                window.occluded = occluded;

                if !occluded {
                    window.request_redraw();
                }

                self.next_redraw = None;
                self.last_update = None;
            }
            WindowEvent::RedrawRequested if active && !window.occluded => {
                let result = (|| -> Result<FrameOutcome> {
                    let size = window.native.inner_size();

                    session.graphics.resize(size.width, size.height)?;

                    if !session.graphics.drawable() {
                        return Ok(FrameOutcome::Retry);
                    }

                    let now = Instant::now();

                    let elapsed = self
                        .last_update
                        .replace(now)
                        .map_or(Duration::ZERO, |previous| now - previous);

                    session
                        .application
                        .update(Update {
                            elapsed,
                            input: &self.input,
                            context: Context {
                                window,
                                graphics: &mut session.graphics,
                            },
                        })
                        .map_err(RuntimeError::Update)?;

                    if window.exit_requested {
                        event_loop.exit();

                        return Ok(FrameOutcome::Retry);
                    }

                    self.input.finish_update();

                    let size = window.native.inner_size();

                    session.graphics.resize(size.width, size.height)?;

                    if !session.graphics.drawable() {
                        return Ok(FrameOutcome::Retry);
                    }

                    session
                        .graphics
                        .render(
                            |frame| session.application.draw(frame),
                            || window.native.pre_present_notify(),
                        )
                        .map_err(RuntimeError::from)
                })();

                match result {
                    Ok(FrameOutcome::Presented) => self.next_redraw = None,
                    Ok(FrameOutcome::Retry) => {
                        self.next_redraw =
                            Instant::now().checked_add(self.settings.runtime.retry_delay);

                        if self.next_redraw.is_none() {
                            self.fail(event_loop, RuntimeError::InvalidRetryDelay);
                        }
                    }
                    Err(error) => self.fail(event_loop, error),
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        event_loop.set_control_flow(ControlFlow::Wait);

        let (session, active) = match &self.lifecycle {
            Lifecycle::Active(session) => (session, true),
            Lifecycle::Suspended(session) => (session, false),
            Lifecycle::Pending(_) | Lifecycle::Stopped => return,
        };

        let window = &session.window;

        if window.exit_requested {
            event_loop.exit();

            return;
        }

        let size = window.native.inner_size();

        if !active || window.occluded || size.width == 0 || size.height == 0 {
            return;
        }

        if let Some(deadline) = self.next_redraw.filter(|&t| t > Instant::now()) {
            event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
        } else if self.settings.runtime.continuous_redraw || self.next_redraw.is_some() {
            window.request_redraw();
        }
    }
}
