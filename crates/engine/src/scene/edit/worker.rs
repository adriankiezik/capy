use super::patch::{Input, Patch};
use crate::{
    scene::{Result, SceneError, connectivity::Connectivity},
    world::VoxelCoord,
};
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{Arc, Mutex, mpsc},
};

enum Job {
    Prepare {
        input: Input,
        voxels: Vec<VoxelCoord>,
        result: mpsc::SyncSender<Result<Patch>>,
    },
    Retire(Box<Patch>),
}

pub(super) struct Workers {
    sender: Option<mpsc::SyncSender<Job>>,
    handles: Vec<std::thread::JoinHandle<()>>,
}

impl Drop for Workers {
    fn drop(&mut self) {
        drop(self.sender.take());

        for handle in self.handles.drain(..) {
            let _ = handle.join();
        }
    }
}

impl Workers {
    pub(super) fn new(count: usize) -> Result<Self> {
        let (sender, receiver) = mpsc::sync_channel::<Job>(count * 2);

        let receiver = Arc::new(Mutex::new(receiver));

        let mut workers = Self {
            sender: Some(sender),
            handles: Vec::new(),
        };

        for index in 0..count {
            let receiver = receiver.clone();

            let handle = std::thread::Builder::new()
                .name(format!("scene-edit-{index}"))
                .spawn(move || {
                    let mut cache = Connectivity::default();

                    loop {
                        let job = receiver
                            .lock()
                            .unwrap_or_else(|error| error.into_inner())
                            .recv();

                        match job {
                            Ok(Job::Prepare {
                                input,
                                voxels,
                                result,
                            }) => {
                                let prepared = catch_unwind(AssertUnwindSafe(|| {
                                    input.prepare(voxels, &mut cache)
                                }))
                                .unwrap_or_else(|_| {
                                    cache = Connectivity::default();

                                    Err(SceneError::EditWorkerStopped)
                                });

                                let _ = result.send(prepared);
                            }
                            Ok(Job::Retire(patch)) => drop(patch),
                            Err(_) => break,
                        }
                    }
                })
                .map_err(SceneError::EditWorker)?;

            workers.handles.push(handle);
        }

        Ok(workers)
    }

    pub(super) fn submit(
        &self,
        input: Input,
        voxels: Vec<VoxelCoord>,
    ) -> Result<Option<mpsc::Receiver<Result<Patch>>>> {
        let (result, receiver) = mpsc::sync_channel(1);

        match self
            .sender
            .as_ref()
            .ok_or(SceneError::EditWorkerStopped)?
            .try_send(Job::Prepare {
                input,
                voxels,
                result,
            }) {
            Ok(()) => Ok(Some(receiver)),
            Err(mpsc::TrySendError::Full(_)) => Ok(None),
            Err(mpsc::TrySendError::Disconnected(_)) => Err(SceneError::EditWorkerStopped),
        }
    }

    pub(super) fn retire(&self, patch: Patch) {
        if let Some(sender) = &self.sender {
            let _ = sender.try_send(Job::Retire(Box::new(patch)));
        }
    }
}
