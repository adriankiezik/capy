use super::{Engine, View};
use crate::assets::Assets;
use crate::graphics::{FrameOutcome, Graphics};
use crate::render::Renderer;
use crate::runtime::{
    ApplicationResult, Input, Result, RuntimeError, Settings, Window, WindowControls,
};
use std::{
    cell::RefCell,
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll, Wake, Waker},
    time::{Duration, Instant},
};
use winit::{
    application::ApplicationHandler,
    event::{DeviceEvent, DeviceId, ElementState, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
    window::WindowId,
};

pub fn run<F: Future<Output = ApplicationResult<()>> + 'static>(
    settings: Settings,
    create: impl FnOnce(Engine) -> F + 'static,
) -> Result<()> {
    if settings.runtime.retry_delay.is_zero()
        || Instant::now()
            .checked_add(settings.runtime.retry_delay)
            .is_none()
    {
        return Err(RuntimeError::InvalidRetryDelay);
    }

    if !(1..=1000).contains(&settings.runtime.tick_rate)
        || settings.runtime.max_ticks_per_update == 0
    {
        return Err(RuntimeError::InvalidTickRate);
    }

    let event_loop = EventLoop::<()>::with_user_event().build()?;

    let clock = SimulationClock::new(
        settings.runtime.tick_rate,
        settings.runtime.max_ticks_per_update,
    );

    let shared = Rc::new(RefCell::new(Shared {
        session: None,
        input: Input::default(),
        active: false,
        closed: false,
        ready: None,
        waiting: None,
        delta: clock.tick,
        revision: 0,
        outcome: None,
    }));

    let assets = Assets::new(settings.assets.clone())?;

    let mut runtime = Runtime {
        settings,
        assets,
        create: Some(create),
        future: None,
        shared,
        wake: Arc::new(RuntimeWake {
            proxy: event_loop.create_proxy(),
            queued: AtomicBool::new(false),
        }),
        next_redraw: None,
        clock,
        error: None,
    };

    event_loop.run_app(&mut runtime)?;

    if let Some(error) = runtime.error {
        return Err(error);
    }

    Ok(())
}

struct RuntimeWake {
    proxy: EventLoopProxy<()>,
    queued: AtomicBool,
}

impl Wake for RuntimeWake {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        if !self.queued.swap(true, Ordering::AcqRel) {
            let _ = self.proxy.send_event(());
        }
    }
}

struct SimulationClock {
    tick: Duration,
    max_ticks_per_update: u32,
    last_update: Option<Instant>,
    accumulated: Duration,
}

impl SimulationClock {
    fn new(tick_rate: u32, max_ticks_per_update: u32) -> Self {
        Self {
            tick: Duration::from_secs_f64(1.0 / tick_rate as f64),
            max_ticks_per_update,
            last_update: None,
            accumulated: Duration::ZERO,
        }
    }

    fn reset(&mut self) {
        self.last_update = None;
        self.accumulated = Duration::ZERO;
    }

    fn advance(&mut self, now: Instant) -> u32 {
        let elapsed = match self.last_update.replace(now) {
            Some(previous) => now - previous,
            None => Duration::ZERO,
        };

        let total = self.accumulated.saturating_add(elapsed);

        let due = total.as_nanos() / self.tick.as_nanos();

        self.accumulated = Duration::from_nanos((total.as_nanos() % self.tick.as_nanos()) as u64);

        due.min(self.max_ticks_per_update as u128) as u32
    }

    fn next_tick(&self) -> Option<Instant> {
        self.last_update
            .and_then(|last| last.checked_add(self.tick - self.accumulated))
    }
}

pub(super) struct Session {
    graphics: Graphics,
    renderer: Renderer,
    pub(super) window: Window,
    pub(super) window_controls: WindowControls,
}

impl Session {
    fn new(event_loop: &ActiveEventLoop, settings: &mut Settings) -> Result<Self> {
        let window = Window::create(event_loop, settings.window.clone())?;

        let size = window.native.inner_size();

        let graphics = pollster::block_on(Graphics::new(
            window.native.clone(),
            size.width,
            size.height,
            std::mem::take(&mut settings.graphics),
        ))?;

        let renderer = Renderer::new(&graphics)?;

        Ok(Self {
            graphics,
            renderer,
            window,
            window_controls: std::mem::take(&mut settings.window_controls),
        })
    }

    fn drawable(&mut self) -> Result<bool> {
        let size = self.window.native.inner_size();

        self.graphics.resize(size.width, size.height)?;

        Ok(self.graphics.drawable())
    }

    pub(super) fn render(
        &mut self,
        view: View<'_>,
        canvas: &crate::ui::Canvas,
    ) -> Result<FrameOutcome> {
        if !self.drawable()? {
            return Ok(FrameOutcome::Retry);
        }

        self.renderer.prepare(
            &self.graphics,
            view.scene,
            view.camera,
            view.selection,
            canvas,
            self.window.native.scale_factor() as f32,
        )?;

        self.graphics
            .render(
                |frame| {
                    self.renderer.draw(frame);

                    Ok(())
                },
                || self.window.native.pre_present_notify(),
            )
            .map_err(RuntimeError::from)
    }
}

pub(super) struct Shared {
    pub(super) session: Option<Session>,
    pub(super) input: Input,
    pub(super) active: bool,
    pub(super) closed: bool,
    pub(super) ready: Option<u32>,
    pub(super) waiting: Option<Waker>,
    pub(super) delta: Duration,
    pub(super) revision: u64,
    pub(super) outcome: Option<FrameOutcome>,
}

impl Shared {
    fn invalidate(&mut self) {
        self.revision = self.revision.wrapping_add(1);
        self.ready = None;
    }
}

struct Runtime<C, F> {
    settings: Settings,
    assets: Assets,
    create: Option<C>,
    future: Option<Pin<Box<F>>>,
    shared: Rc<RefCell<Shared>>,
    wake: Arc<RuntimeWake>,
    next_redraw: Option<Instant>,
    clock: SimulationClock,
    error: Option<RuntimeError>,
}

impl<C, F: Future<Output = ApplicationResult<()>>> Runtime<C, F> {
    fn fail(&mut self, event_loop: &ActiveEventLoop, error: RuntimeError) {
        self.error = Some(error);
        self.shared.borrow_mut().closed = true;
        self.future = None;

        event_loop.exit();
    }

    fn close(&mut self, event_loop: &ActiveEventLoop) {
        self.shared.borrow_mut().closed = true;

        self.poll(event_loop);

        self.future = None;

        event_loop.exit();
    }

    fn poll(&mut self, event_loop: &ActiveEventLoop) {
        let Some(future) = self.future.as_mut() else {
            return;
        };

        let waker = Waker::from(self.wake.clone());

        match future.as_mut().poll(&mut Context::from_waker(&waker)) {
            Poll::Ready(Ok(())) => {
                self.future = None;
                self.shared.borrow_mut().closed = true;

                event_loop.exit();
            }
            Poll::Ready(Err(error)) => self.fail(event_loop, RuntimeError::Application(error)),
            Poll::Pending => {}
        }
    }

    fn finish_frame(&mut self) -> Result<()> {
        match self.shared.borrow_mut().outcome.take() {
            Some(FrameOutcome::Presented) => self.next_redraw = None,
            Some(FrameOutcome::Retry) => {
                self.next_redraw = Some(
                    Instant::now()
                        .checked_add(self.settings.runtime.retry_delay)
                        .ok_or(RuntimeError::InvalidRetryDelay)?,
                );
            }
            None => {}
        }

        Ok(())
    }

    fn redraw(&mut self, event_loop: &ActiveEventLoop) -> Result<()> {
        {
            let mut shared = self.shared.borrow_mut();

            if !shared.active || shared.closed || shared.waiting.is_none() {
                return Ok(());
            }

            let Some(session) = shared.session.as_mut() else {
                return Ok(());
            };

            if session.window.occluded {
                return Ok(());
            }

            if !session.drawable()? {
                shared.outcome = Some(FrameOutcome::Retry);
            } else {
                shared.ready = Some(self.clock.advance(Instant::now()));
            }
        }

        if self.shared.borrow().ready.is_some() {
            self.poll(event_loop);
        }

        self.finish_frame()
    }
}

impl<C, F> ApplicationHandler<()> for Runtime<C, F>
where
    C: FnOnce(Engine) -> F,
    F: Future<Output = ApplicationResult<()>>,
{
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let result = (|| -> Result<()> {
            let mut shared = self.shared.borrow_mut();

            if shared.closed || shared.active {
                return Ok(());
            }

            if let Some(session) = shared.session.as_mut() {
                let size = session.window.native.inner_size();

                session
                    .graphics
                    .attach(session.window.native.clone(), size.width, size.height)?;
            } else {
                shared.session = Some(Session::new(event_loop, &mut self.settings)?);
            }

            let session = shared.session.as_mut().ok_or(RuntimeError::NotRunning)?;

            session.window.occluded = false;

            let focused = session.window.native.has_focus();

            session.window.request_redraw();

            shared.input.focus(focused);

            shared.active = true;

            shared.invalidate();

            Ok(())
        })();

        if let Err(error) = result {
            self.fail(event_loop, error);

            return;
        }

        self.clock.reset();

        self.next_redraw = None;

        if let Some(create) = self.create.take() {
            self.future = Some(Box::pin(create(Engine {
                shared: self.shared.clone(),
                assets: self.assets.clone(),
            })));
        }

        self.poll(event_loop);
    }

    fn suspended(&mut self, _: &ActiveEventLoop) {
        let mut shared = self.shared.borrow_mut();

        if let Some(session) = shared.session.as_mut() {
            session.window.release_cursor();

            session.graphics.detach();
        }

        shared.active = false;
        shared.input = Input::default();

        shared.invalidate();

        self.clock.reset();

        self.next_redraw = None;
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, _: ()) {
        self.wake.queued.store(false, Ordering::Release);

        self.poll(event_loop);

        if let Err(error) = self.finish_frame() {
            self.fail(event_loop, error);
        }
    }

    fn device_event(&mut self, _: &ActiveEventLoop, _: DeviceId, event: DeviceEvent) {
        let mut shared = self.shared.borrow_mut();

        if !shared.active {
            return;
        }

        let Shared { session, input, .. } = &mut *shared;

        if let DeviceEvent::MouseMotion { delta } = event
            && let Some(session) = session
        {
            input.sync_cursor(
                session.window.cursor_captured(),
                session.window.capture_revision,
            );

            input.motion(delta);
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if self
            .shared
            .borrow()
            .session
            .as_ref()
            .is_none_or(|s| s.window.native.id() != id)
        {
            return;
        }

        if matches!(event, WindowEvent::RedrawRequested) {
            if let Err(error) = self.redraw(event_loop) {
                self.fail(event_loop, error);
            }

            return;
        }

        let result = (|| -> Result<bool> {
            let mut shared = self.shared.borrow_mut();

            let Shared { session, input, .. } = &mut *shared;

            let session = session.as_mut().ok_or(RuntimeError::NotRunning)?;

            let window = &mut session.window;

            match event {
                WindowEvent::CloseRequested => {
                    window.close_requested = true;

                    if self.settings.runtime.close_on_request {
                        return Ok(true);
                    }

                    window.request_redraw();
                }
                WindowEvent::KeyboardInput { event, .. } => {
                    let key = event.physical_key;

                    let down = event.state == ElementState::Pressed;

                    input.change(key, down);

                    if self.settings.runtime.exit_key == Some(key) && down {
                        return Ok(true);
                    }

                    window.request_redraw();
                }
                WindowEvent::MouseInput { state, button, .. } => {
                    input.change(button, state == ElementState::Pressed);

                    window.request_redraw();
                }
                WindowEvent::Focused(focused) => {
                    input.focus(focused);

                    if !focused {
                        window.release_cursor();

                        shared.invalidate();

                        self.clock.reset();
                    }
                }
                WindowEvent::Resized(size) => {
                    session.graphics.resize(size.width, size.height)?;

                    window.request_redraw();

                    self.next_redraw = None;

                    if size.width == 0 || size.height == 0 {
                        self.clock.reset();

                        shared.invalidate();
                    }
                }
                WindowEvent::Occluded(occluded) => {
                    window.occluded = occluded;

                    if !occluded {
                        window.request_redraw();
                    }

                    shared.invalidate();

                    self.next_redraw = None;

                    self.clock.reset();
                }
                _ => {}
            }

            Ok(false)
        })();

        match result {
            Ok(true) => self.close(event_loop),
            Ok(false) => {}
            Err(error) => self.fail(event_loop, error),
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        event_loop.set_control_flow(ControlFlow::Wait);

        let shared = self.shared.borrow();

        let Some(session) = shared.session.as_ref() else {
            return;
        };

        let window = &session.window;

        if window.exit_requested {
            drop(shared);

            self.close(event_loop);

            return;
        }

        let size = window.native.inner_size();

        if shared.closed
            || !shared.active
            || window.occluded
            || size.width == 0
            || size.height == 0
            || shared.waiting.is_none()
        {
            return;
        }

        if let Some(deadline) = self.next_redraw.filter(|&t| t > Instant::now()) {
            event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
        } else if self.settings.runtime.continuous_redraw || self.next_redraw.is_some() {
            window.request_redraw();
        } else if let Some(deadline) = self.clock.next_tick().filter(|&t| t > Instant::now()) {
            event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
        } else {
            window.request_redraw();
        }
    }
}
