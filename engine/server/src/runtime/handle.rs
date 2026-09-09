use super::{Error, Result};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
};

pub struct ServerHandle {
    pub(super) stop: Arc<AtomicBool>,
    pub(super) thread: Option<JoinHandle<Result<()>>>,
}

impl ServerHandle {
    pub fn shutdown(&self) {
        self.stop.store(true, Ordering::Release);

        if let Some(thread) = &self.thread {
            thread.thread().unpark();
        }
    }

    pub fn finished(&self) -> bool {
        self.thread.as_ref().is_none_or(JoinHandle::is_finished)
    }

    pub fn join(mut self) -> Result<()> {
        self.thread
            .take()
            .map_or(Ok(()), |thread| thread.join().map_err(|_| Error::Thread)?)
    }
}

impl Drop for ServerHandle {
    fn drop(&mut self) {
        self.shutdown();

        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
