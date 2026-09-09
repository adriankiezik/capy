use std::{
    future::poll_fn,
    task::Poll,
    time::{Duration, Instant},
};

pub(super) struct Work {
    deadline: Instant,
}

impl Default for Work {
    fn default() -> Self {
        Self {
            deadline: Instant::now() + Self::QUANTUM,
        }
    }
}

impl Work {
    const QUANTUM: Duration = Duration::from_millis(1);

    pub(super) async fn checkpoint(&mut self) {
        if Instant::now() < self.deadline {
            return;
        }

        let mut yielded = false;

        poll_fn(|cx| {
            if yielded {
                Poll::Ready(())
            } else {
                yielded = true;

                cx.waker().wake_by_ref();

                Poll::Pending
            }
        })
        .await;

        self.deadline = Instant::now() + Self::QUANTUM;
    }
}
