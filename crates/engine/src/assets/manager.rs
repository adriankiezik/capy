use super::{Asset, AssetError, AssetSettings, AssetSource, Handle, Result, source::normalize};
use std::{
    any::{Any, TypeId},
    collections::HashMap,
    future::poll_fn,
    panic::{AssertUnwindSafe, catch_unwind},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock, Weak, mpsc},
    task::{Poll, Waker},
    thread,
};

type Job = Box<dyn FnOnce() + Send>;

type Cache = HashMap<(TypeId, PathBuf), Weak<dyn Any + Send + Sync>>;

pub(super) struct Entry<T: Asset> {
    result: OnceLock<Result<Arc<T>>>,
    waiting: Mutex<Vec<Waker>>,
}

impl<T: Asset> Entry<T> {
    fn complete(&self, result: Result<Arc<T>>) {
        let waiting = {
            let mut waiting = self
                .waiting
                .lock()
                .unwrap_or_else(|error| error.into_inner());

            let _ = self.result.set(result);

            std::mem::take(&mut *waiting)
        };

        for waker in waiting {
            waker.wake();
        }
    }

    async fn ready(self: Arc<Self>) -> Result<Handle<T>> {
        let value = poll_fn(|cx| {
            let mut waiting = self
                .waiting
                .lock()
                .unwrap_or_else(|error| error.into_inner());

            if let Some(result) = self.result.get() {
                return Poll::Ready(result.clone());
            }

            if !waiting.iter().any(|waker| waker.will_wake(cx.waker())) {
                waiting.push(cx.waker().clone());
            }

            Poll::Pending
        })
        .await?;

        Ok(Handle { value, entry: self })
    }
}

struct Shared {
    source: Arc<dyn AssetSource>,
    cache: Mutex<Cache>,
    jobs: mpsc::Sender<Job>,
}

#[derive(Clone)]
pub struct Assets {
    shared: Arc<Shared>,
}

impl Assets {
    pub fn new(settings: AssetSettings) -> Result<Self> {
        if settings.workers == 0 {
            return Err(AssetError::InvalidWorkerCount);
        }

        let (jobs, receiver) = mpsc::channel::<Job>();

        let receiver = Arc::new(Mutex::new(receiver));

        for index in 0..settings.workers {
            let receiver = receiver.clone();

            thread::Builder::new()
                .name(format!("asset-loader-{index}"))
                .spawn(move || {
                    loop {
                        let job = receiver
                            .lock()
                            .unwrap_or_else(|error| error.into_inner())
                            .recv();

                        match job {
                            Ok(job) => job(),
                            Err(_) => break,
                        }
                    }
                })
                .map_err(|error| AssetError::WorkerUnavailable(error.to_string()))?;
        }

        Ok(Self {
            shared: Arc::new(Shared {
                source: settings.source,
                cache: Mutex::new(HashMap::new()),
                jobs,
            }),
        })
    }

    pub async fn load<T: Asset>(&self, path: impl AsRef<Path>) -> Result<Handle<T>> {
        let path = normalize(path.as_ref())?;

        let key = (TypeId::of::<T>(), path.clone());

        let entry = {
            let mut cache = self
                .shared
                .cache
                .lock()
                .unwrap_or_else(|error| error.into_inner());

            cache.retain(|_, entry| entry.strong_count() > 0);

            if let Some(entry) = cache
                .get(&key)
                .and_then(Weak::upgrade)
                .and_then(|entry| entry.downcast::<Entry<T>>().ok())
            {
                entry
            } else {
                let entry = Arc::new(Entry::<T> {
                    result: OnceLock::new(),
                    waiting: Mutex::new(Vec::new()),
                });

                let erased: Arc<dyn Any + Send + Sync> = entry.clone();

                cache.insert(key, Arc::downgrade(&erased));

                let source = self.shared.source.clone();

                let pending = entry.clone();

                let job = Box::new(move || {
                    let result = catch_unwind(AssertUnwindSafe(|| {
                        let bytes = source.read(&path).map_err(|source| AssetError::Io {
                            path: path.clone(),
                            source: Arc::new(source),
                        })?;

                        T::decode(bytes)
                            .map(Arc::new)
                            .map_err(|error| AssetError::Decode {
                                path: path.clone(),
                                message: format!("{error:#}"),
                            })
                    }))
                    .unwrap_or_else(|_| Err(AssetError::LoaderPanicked(path)));

                    pending.complete(result);
                });

                if self.shared.jobs.send(job).is_err() {
                    entry.complete(Err(AssetError::WorkerUnavailable(
                        "Asset queue disconnected".to_owned(),
                    )));
                }

                entry
            }
        };

        entry.ready().await
    }
}
