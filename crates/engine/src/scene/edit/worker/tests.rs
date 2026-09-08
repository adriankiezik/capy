#![allow(clippy::unwrap_used)]

use super::*;
use std::{future::poll_fn, time::Duration};

const TIMEOUT: Duration = Duration::from_secs(5);

fn submit(
    workers: &Workers,
    future: impl Future<Output = Result<Patch>> + Send + 'static,
) -> Receipt {
    let (result, receiver) = mpsc::sync_channel(1);

    let task = Arc::new(Task {
        state: Mutex::new(State::Running(Box::pin(async move {
            (Connectivity::default(), future.await)
        }))),
        result,
        queue: Arc::downgrade(&workers.queue),
        queued: AtomicBool::new(false),
        cancelled: AtomicBool::new(false),
        finished: AtomicBool::new(false),
    });

    workers.queue.0.lock().unwrap().active += 1;

    task.schedule();

    Receipt {
        task,
        receiver: Some(receiver),
    }
}

fn receive(receipt: &Receipt) -> Result<Patch> {
    receipt
        .receiver
        .as_ref()
        .unwrap()
        .recv_timeout(TIMEOUT)
        .unwrap()
}

struct Suspended {
    started: Option<mpsc::SyncSender<()>>,
    dropped: mpsc::SyncSender<String>,
}

impl Future for Suspended {
    type Output = Result<Patch>;

    fn poll(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        if let Some(started) = self.started.take() {
            started.send(()).unwrap();
        }

        Poll::Pending
    }
}

impl Drop for Suspended {
    fn drop(&mut self) {
        let _ = self
            .dropped
            .send(std::thread::current().name().unwrap_or_default().to_owned());
    }
}

// Cancels a background job after it has started. Checks that its cleanup also
// happens in the background, rather than on the main thread used by the game,
// and that the system can still complete another job afterward.
#[test]
fn cancellation_releases_suspended_work_on_a_worker() {
    for count in [1, 4] {
        let workers = Workers::new(count).unwrap();

        let (started, start) = mpsc::sync_channel(1);

        let (dropped, drop) = mpsc::sync_channel(1);

        let receipt = submit(
            &workers,
            Suspended {
                started: Some(started),
                dropped,
            },
        );

        start.recv_timeout(TIMEOUT).unwrap();

        std::mem::drop(receipt);

        assert!(
            drop.recv_timeout(TIMEOUT)
                .unwrap()
                .starts_with("scene-edit-")
        );

        assert!(matches!(
            receive(&submit(&workers, async { Ok(Patch::Unchanged) })),
            Ok(Patch::Unchanged)
        ));
    }
}

// Fills all available places for background work, then cancels those jobs.
// Checks that the freed places can be used by a new job and it finishes normally.
#[test]
fn capacity_recovers_after_cancellation() {
    let workers = Workers::new(1).unwrap();

    let mut receipts = Vec::new();

    let (dropped, drops) = mpsc::sync_channel(2);

    for _ in 0..2 {
        let (started, start) = mpsc::sync_channel(1);

        receipts.push(submit(
            &workers,
            Suspended {
                started: Some(started),
                dropped: dropped.clone(),
            },
        ));

        start.recv_timeout(TIMEOUT).unwrap();
    }

    let scene = crate::scene::Scene::new(crate::scene::tests::settings()).unwrap();

    let input = || Input::capture(&scene, crate::scene::edit::Domain::Static).unwrap();

    assert!(
        workers
            .submit(input(), Vec::new(), false)
            .unwrap()
            .is_none()
    );

    drop(receipts);

    for _ in 0..2 {
        drops.recv_timeout(TIMEOUT).unwrap();
    }

    let deadline = std::time::Instant::now() + TIMEOUT;

    let receipt = loop {
        if let Some(receipt) = workers.submit(input(), Vec::new(), false).unwrap() {
            break receipt;
        }

        assert!(std::time::Instant::now() < deadline);

        std::thread::yield_now();
    };

    assert!(matches!(receive(&receipt), Ok(Patch::Unchanged)));
}

// Repeatedly tells each background job to continue, including duplicate reminders.
// Checks that every job produces only one result, then deliberately crashes one
// job and checks that later jobs can still finish and all job slots are released.
#[test]
fn duplicate_wakes_complete_once_and_panics_do_not_stop_workers() {
    for count in [1, 4] {
        let workers = Workers::new(count).unwrap();

        let receipts: Vec<_> = (0..32)
            .map(|_| {
                let mut polls = 0;

                submit(
                    &workers,
                    poll_fn(move |cx| {
                        polls += 1;

                        for _ in 0..8 {
                            cx.waker().wake_by_ref();
                        }

                        if polls == 32 {
                            Poll::Ready(Ok(Patch::Unchanged))
                        } else {
                            Poll::Pending
                        }
                    }),
                )
            })
            .collect();

        for receipt in &receipts {
            assert!(matches!(receive(receipt), Ok(Patch::Unchanged)));

            assert!(matches!(receipt.try_recv(), Err(mpsc::TryRecvError::Empty)));
        }

        let failed = submit(&workers, async {
            std::panic::resume_unwind(Box::new("preparation failure"))
        });

        assert!(matches!(
            receive(&failed),
            Err(SceneError::EditWorkerStopped)
        ));

        assert!(matches!(
            receive(&submit(&workers, async { Ok(Patch::Unchanged) })),
            Ok(Patch::Unchanged)
        ));

        drop(receipts);

        drop(failed);

        let queue = workers.queue.clone();

        drop(workers);

        assert_eq!(queue.0.lock().unwrap().active, 0);
    }
}

// Runs a job that continually asks for more time alongside a short job.
// Checks that the short job still finishes and that shutting down stops the
// endless job instead of waiting forever for it to finish by itself.
#[test]
fn yielding_work_is_fair_and_shutdown_cancels_it() {
    let (done, completion) = mpsc::sync_channel(1);

    let handle = std::thread::spawn(move || {
        let workers = Workers::new(1).unwrap();

        let busy = submit(
            &workers,
            poll_fn(|cx| {
                cx.waker().wake_by_ref();

                Poll::Pending
            }),
        );

        assert!(matches!(
            receive(&submit(&workers, async { Ok(Patch::Unchanged) })),
            Ok(Patch::Unchanged)
        ));

        drop(workers);

        assert!(busy.task.finished.load(Ordering::Acquire));

        done.send(()).unwrap();
    });

    completion.recv_timeout(TIMEOUT).unwrap();

    handle.join().unwrap();
}
