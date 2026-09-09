use super::driver::Shared;
use crate::{
    assets::Assets,
    camera::Camera,
    replica::{Hit, Replica},
    runtime::{Input, Result, RuntimeError, Window},
    ui::Canvas,
};
use std::{cell::RefCell, future::poll_fn, rc::Rc, task::Poll, time::Duration};

pub struct Engine {
    pub(super) shared: Rc<RefCell<Shared>>,
    pub(super) assets: Assets,
}

struct FrameWait<'a> {
    shared: &'a RefCell<Shared>,
}

impl Drop for FrameWait<'_> {
    fn drop(&mut self) {
        let mut shared = self.shared.borrow_mut();

        shared.waiting = None;
        shared.ready = None;
    }
}

pub struct InputUpdate {
    pub delta: Duration,
    pub input: Input,
}

pub struct Frame<'a> {
    engine: &'a mut Engine,
    remaining_ticks: u32,
    delta: Duration,
    revision: u64,
    canvas: Canvas,
}

pub struct View<'a> {
    pub scene: &'a Replica,
    pub camera: Camera,
    pub selection: Option<Hit>,
}

impl<'a> View<'a> {
    pub fn new(scene: &'a Replica, camera: Camera) -> Self {
        Self {
            scene,
            camera,
            selection: None,
        }
    }

    #[must_use]
    pub fn with_selection(mut self, selection: Option<Hit>) -> Self {
        self.selection = selection;

        self
    }
}

impl Engine {
    pub fn assets(&self) -> Assets {
        self.assets.clone()
    }

    pub async fn next_frame(&mut self) -> Result<Option<Frame<'_>>> {
        let waiting = FrameWait {
            shared: &self.shared,
        };

        let ready = poll_fn(|cx| {
            let mut shared = waiting.shared.borrow_mut();

            if shared.closed {
                return Poll::Ready(None);
            }

            if let Some(ticks) = shared.ready.take() {
                shared.waiting = None;

                return Poll::Ready(Some((ticks, shared.delta, shared.revision)));
            }

            shared.waiting = Some(cx.waker().clone());

            Poll::Pending
        })
        .await;

        drop(waiting);

        Ok(ready.map(|(remaining_ticks, delta, revision)| Frame {
            engine: self,
            remaining_ticks,
            delta,
            revision,
            canvas: Canvas::new(),
        }))
    }

    pub fn with_window<T>(&mut self, access: impl FnOnce(&mut Window) -> T) -> Result<T> {
        let mut shared = self.shared.borrow_mut();

        let session = shared.session.as_mut().ok_or(RuntimeError::NotRunning)?;

        Ok(access(&mut session.window))
    }
}

impl Frame<'_> {
    pub fn canvas(&mut self) -> &mut Canvas {
        &mut self.canvas
    }

    pub fn take_captured_mouse_delta(&mut self) -> Result<glam::Vec2> {
        let mut shared = self.engine.shared.borrow_mut();

        if shared.closed || self.revision != shared.revision {
            return Ok(glam::Vec2::ZERO);
        }

        shared.update_window_controls()?;

        Ok(shared.input.take_captured_mouse_delta())
    }

    pub fn next_input(&mut self) -> Result<Option<InputUpdate>> {
        let mut shared = self.engine.shared.borrow_mut();

        if self.remaining_ticks == 0 || shared.closed || self.revision != shared.revision {
            return Ok(None);
        }

        shared.update_window_controls()?;

        let Shared { session, input, .. } = &mut *shared;

        let session = session.as_mut().ok_or(RuntimeError::NotRunning)?;

        if session.window.exit_requested {
            input.finish_update();

            return Ok(None);
        }

        self.remaining_ticks -= 1;

        let tick = InputUpdate {
            delta: self.delta,
            input: input.clone(),
        };

        input.finish_update();

        Ok(Some(tick))
    }

    pub fn with_window<T>(&mut self, access: impl FnOnce(&mut Window) -> T) -> Result<T> {
        self.engine.with_window(access)
    }

    pub fn present(self, view: View<'_>) -> Result<()> {
        let mut shared = self.engine.shared.borrow_mut();

        if shared.closed || !shared.active || self.revision != shared.revision {
            return Ok(());
        }

        let session = shared.session.as_mut().ok_or(RuntimeError::NotRunning)?;

        if session.window.exit_requested {
            return Ok(());
        }

        shared.outcome = Some(session.render(view, &self.canvas)?);

        Ok(())
    }
}
