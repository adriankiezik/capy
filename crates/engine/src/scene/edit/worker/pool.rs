#[cfg(test)]
#[path = "tests.rs"]
mod tests;

use super::super::patch::{Input, Patch};
use crate::{
    scene::{Result, SceneError, connectivity::Connectivity},
    world::VoxelCoord,
};
use std::{
    collections::VecDeque,
    future::Future,
    panic::{AssertUnwindSafe, catch_unwind},
    pin::Pin,
    sync::{
        Arc, Condvar, Mutex, Weak,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    task::{Context, Poll, Wake, Waker},
};

type Preparation = Pin<Box<dyn Future<Output = (Connectivity, Result<Patch>)> + Send>>;

type Shared = (Mutex<Queue>, Condvar);

enum State {
    Start {
        input: Input,
        voxels: Vec<VoxelCoord>,
        structural: bool,
    },
    Running(Preparation),
    Finished,
}

struct Task {
    state: Mutex<State>,
    result: mpsc::SyncSender<Result<Patch>>,
    queue: Weak<Shared>,
    queued: AtomicBool,
    cancelled: AtomicBool,
    finished: AtomicBool,
}

impl Task {
    fn schedule(self: &Arc<Self>) {
        if self.finished.load(Ordering::Acquire) || self.queued.swap(true, Ordering::AcqRel) {
            return;
        }

        if let Some(shared) = self.queue.upgrade() {
            shared
                .0
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .jobs
                .push_back(Job::Run(self.clone()));

            shared.1.notify_one();
        }
    }

    fn poll(self: &Arc<Self>, cache: &mut Connectivity) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());

        self.queued.store(false, Ordering::Release);

        if matches!(*state, State::Finished) {
            return;
        }

        if !self.cancelled.load(Ordering::Acquire) {
            let prepared = catch_unwind(AssertUnwindSafe(|| {
                if matches!(*state, State::Start { .. }) {
                    let State::Start {
                        input,
                        voxels,
                        structural,
                    } = std::mem::replace(&mut *state, State::Finished)
                    else {
                        unreachable!()
                    };

                    let mut local = std::mem::take(cache);

                    *state = State::Running(Box::pin(async move {
                        let prepared = if structural {
                            input.prepare(voxels, &mut local).await
                        } else {
                            input.prepare_local(voxels, &mut local).await
                        };

                        (local, prepared)
                    }));
                }

                let State::Running(task) = &mut *state else {
                    unreachable!()
                };

                task.as_mut()
                    .poll(&mut Context::from_waker(&Waker::from(self.clone())))
            }));

            match prepared {
                Ok(Poll::Pending) => return,
                Ok(Poll::Ready((returned, prepared))) => {
                    *cache = returned;

                    let _ = self.result.send(prepared);
                }
                Err(_) => {
                    let _ = self.result.send(Err(SceneError::EditWorkerStopped));
                }
            }
        }

        self.finished.store(true, Ordering::Release);

        *state = State::Finished;

        if let Some(shared) = self.queue.upgrade() {
            shared
                .0
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .active -= 1;
        }
    }
}

impl Wake for Task {
    fn wake(self: Arc<Self>) {
        self.schedule();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.schedule();
    }
}

pub(in crate::scene::edit) struct Receipt {
    task: Arc<Task>,
    receiver: Option<mpsc::Receiver<Result<Patch>>>,
}

impl Receipt {
    pub(in crate::scene::edit) fn try_recv(
        &self,
    ) -> std::result::Result<Result<Patch>, mpsc::TryRecvError> {
        self.receiver
            .as_ref()
            .ok_or(mpsc::TryRecvError::Disconnected)?
            .try_recv()
    }
}

impl Drop for Receipt {
    fn drop(&mut self) {
        self.task.cancelled.store(true, Ordering::Release);

        self.task.schedule();

        if let Some(shared) = self.task.queue.upgrade()
            && let Some(receiver) = self.receiver.take()
        {
            shared
                .0
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .jobs
                .push_back(Job::Discard(receiver));

            shared.1.notify_one();
        }
    }
}

enum Job {
    Run(Arc<Task>),
    Retire(Box<Patch>),
    Discard(mpsc::Receiver<Result<Patch>>),
}

#[derive(Default)]
struct Queue {
    jobs: VecDeque<Job>,
    active: usize,
    closing: bool,
}

pub(in crate::scene::edit) struct Workers {
    queue: Arc<Shared>,
    capacity: usize,
    handles: Vec<std::thread::JoinHandle<()>>,
}

impl Drop for Workers {
    fn drop(&mut self) {
        self.queue
            .0
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .closing = true;

        self.queue.1.notify_all();

        for handle in self.handles.drain(..) {
            let _ = handle.join();
        }
    }
}

impl Workers {
    pub(in crate::scene::edit) fn new(count: usize) -> Result<Self> {
        let mut workers = Self {
            queue: Arc::new((Mutex::new(Queue::default()), Condvar::new())),
            capacity: count * 2,
            handles: Vec::new(),
        };

        for index in 0..count {
            let shared = workers.queue.clone();

            let handle = std::thread::Builder::new()
                .name(format!("scene-edit-{index}"))
                .spawn(move || {
                    let mut cache = Connectivity::default();

                    loop {
                        let job = {
                            let mut queue =
                                shared.0.lock().unwrap_or_else(|error| error.into_inner());

                            loop {
                                if let Some(job) = queue.jobs.pop_front() {
                                    if queue.closing
                                        && let Job::Run(task) = &job
                                    {
                                        task.cancelled.store(true, Ordering::Release);
                                    }

                                    break job;
                                }

                                if queue.closing {
                                    return;
                                }

                                queue = shared
                                    .1
                                    .wait(queue)
                                    .unwrap_or_else(|error| error.into_inner());
                            }
                        };

                        match job {
                            Job::Run(task) => task.poll(&mut cache),
                            Job::Retire(patch) => drop(patch),
                            Job::Discard(receiver) => drop(receiver),
                        }
                    }
                })
                .map_err(SceneError::EditWorker)?;

            workers.handles.push(handle);
        }

        Ok(workers)
    }

    pub(in crate::scene::edit) fn submit(
        &self,
        input: Input,
        voxels: Vec<VoxelCoord>,
        structural: bool,
    ) -> Result<Option<Receipt>> {
        let mut queue = self
            .queue
            .0
            .lock()
            .unwrap_or_else(|error| error.into_inner());

        if queue.closing {
            return Err(SceneError::EditWorkerStopped);
        }

        if queue.active >= self.capacity {
            return Ok(None);
        }

        let (result, receiver) = mpsc::sync_channel(1);

        let task = Arc::new(Task {
            state: Mutex::new(State::Start {
                input,
                voxels,
                structural,
            }),
            result,
            queue: Arc::downgrade(&self.queue),
            queued: AtomicBool::new(true),
            cancelled: AtomicBool::new(false),
            finished: AtomicBool::new(false),
        });

        queue.active += 1;

        queue.jobs.push_back(Job::Run(task.clone()));

        self.queue.1.notify_one();

        Ok(Some(Receipt {
            task,
            receiver: Some(receiver),
        }))
    }

    pub(in crate::scene::edit) fn retire(&self, patch: Patch) {
        self.queue
            .0
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .jobs
            .push_back(Job::Retire(Box::new(patch)));

        self.queue.1.notify_one();
    }
}
