use super::{
    MeshError, Result,
    geometry::{Mesh, MeshSource, Scratch},
};
use crate::replica::world::{Leaf, WorldRead};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    },
    thread::JoinHandle,
};

struct Job {
    key: [i32; 3],
    source: Arc<MeshSource>,
    world: WorldRead,
    leaves: im::OrdMap<[i32; 3], Arc<Leaf>>,
    edge: i32,
    maximum: usize,
}

pub(crate) struct Scheduler {
    sender: Mutex<Option<mpsc::SyncSender<Job>>>,
    stopped: Arc<AtomicBool>,
    resident: Arc<AtomicUsize>,
    worker: Option<JoinHandle<()>>,
}

impl Scheduler {
    pub(crate) fn new() -> Result<Self> {
        let (sender, receiver) = mpsc::sync_channel::<Job>(8);

        let stopped = Arc::new(AtomicBool::new(false));

        let stop = stopped.clone();

        let resident = Arc::new(AtomicUsize::new(0));

        let allocation = resident.clone();

        let worker = std::thread::Builder::new()
            .name("client-mesh".into())
            .spawn(move || {
                let mut scratch = Scratch::default();

                while let Ok(job) = receiver.recv() {
                    if stop.load(Ordering::Acquire) {
                        break;
                    }

                    let result = job.source.resolve(
                        job.key,
                        job.edge,
                        |key| job.leaves.get(&key).map(Arc::as_ref),
                        &job.world,
                        job.maximum,
                        &mut scratch,
                    );

                    match result {
                        Ok(mesh) => {
                            match allocation.fetch_update(
                                Ordering::AcqRel,
                                Ordering::Acquire,
                                |count| {
                                    count
                                        .checked_add(mesh.vertices.len())
                                        .filter(|count| *count <= job.maximum.saturating_mul(2))
                                },
                            ) {
                                Ok(_) => {
                                    let _ = mesh.allocation.set(allocation.clone());

                                    let _ = job.source.mesh.set(mesh);
                                }
                                Err(count) => {
                                    *job.source.blocked.lock().unwrap_or_else(|e| e.into_inner()) =
                                        Some((count, job.maximum));
                                }
                            }
                        }
                        Err(_) => {
                            job.source.failed.store(true, Ordering::Release);
                        }
                    }

                    job.source.queued.store(false, Ordering::Release);
                }
            })
            .map_err(|_| MeshError::Worker)?;

        Ok(Self {
            sender: Mutex::new(Some(sender)),
            stopped,
            resident,
            worker: Some(worker),
        })
    }

    pub(crate) fn mesh(
        &self,
        key: [i32; 3],
        source: &Arc<MeshSource>,
        world: &WorldRead,
        leaves: &im::OrdMap<[i32; 3], Arc<Leaf>>,
        edge: i32,
        maximum: usize,
    ) -> Option<Arc<Mesh>> {
        if let Some(mesh) = source.ready() {
            return Some(mesh);
        }

        if source
            .blocked
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_some_and(|(count, limit)| {
                self.resident.load(Ordering::Acquire) >= count && maximum <= limit
            })
        {
            return source.fallback();
        }

        if source.failed.load(Ordering::Acquire) || source.queued.swap(true, Ordering::AcqRel) {
            return source.fallback();
        }

        let sender = self.sender.lock().unwrap_or_else(|e| e.into_inner());

        let job = Job {
            key,
            source: source.clone(),
            world: world.clone(),
            leaves: leaves.clone(),
            edge,
            maximum,
        };

        if sender
            .as_ref()
            .is_none_or(|sender| sender.try_send(job).is_err())
        {
            source.queued.store(false, Ordering::Release);
        }

        source.fallback()
    }
}

impl Drop for Scheduler {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Release);

        self.sender.lock().unwrap_or_else(|e| e.into_inner()).take();

        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
