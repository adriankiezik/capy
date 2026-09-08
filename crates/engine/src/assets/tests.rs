#![allow(clippy::unwrap_used)]

use super::*;
use std::{
    future::Future,
    io,
    path::{Path, PathBuf},
    pin::Pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    task::{Context, Poll, Wake, Waker},
    time::Duration,
};

const TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug)]
struct ControlledSource {
    started: mpsc::Sender<PathBuf>,
    release: Mutex<mpsc::Receiver<Vec<u8>>>,
    reads: Arc<AtomicUsize>,
}

impl AssetSource for ControlledSource {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        self.reads.fetch_add(1, Ordering::SeqCst);

        self.started.send(path.to_owned()).unwrap();

        self.release
            .lock()
            .unwrap()
            .recv_timeout(TIMEOUT)
            .map_err(io::Error::other)
    }
}

struct Notify(mpsc::Sender<()>);

impl Wake for Notify {
    fn wake(self: Arc<Self>) {
        let _ = self.0.send(());
    }
}

fn poll<T>(future: Pin<&mut impl Future<Output = T>>, waker: &Waker) -> Poll<T> {
    future.poll(&mut Context::from_waker(waker))
}

fn ready<T>(future: Pin<&mut impl Future<Output = T>>, waker: &Waker) -> T {
    let result = poll(future, waker);

    assert!(
        result.is_ready(),
        "future was not ready after completion notification"
    );

    match result {
        Poll::Ready(value) => value,
        Poll::Pending => unreachable!(),
    }
}

// Requests the same game resource several times while it is still loading. It should
// be read and prepared only once, and cancelling one request must not affect the others.
// Once nobody is using it, it should be released and loaded afresh next time; requesting
// the same file as a different kind of resource must not reuse the wrong prepared result.
#[test]
fn overlapping_loads_share_work_survive_cancellation_and_release_weak_cache() {
    static DECODES: AtomicUsize = AtomicUsize::new(0);

    struct Decoded(Vec<u8>);

    impl Asset for Decoded {
        fn decode(bytes: Vec<u8>) -> anyhow::Result<Self> {
            DECODES.fetch_add(1, Ordering::SeqCst);

            Ok(Self(bytes))
        }
    }

    let (started_tx, started) = mpsc::channel();

    let (release, release_rx) = mpsc::channel();

    let reads = Arc::new(AtomicUsize::new(0));

    let assets = Assets::new(AssetSettings::default().with_workers(1).with_source(
        ControlledSource {
            started: started_tx,
            release: Mutex::new(release_rx),
            reads: reads.clone(),
        },
    ))
    .unwrap();

    let (wake_tx, wakes) = mpsc::channel();

    let wakers: Vec<_> = (0..3)
        .map(|_| Waker::from(Arc::new(Notify(wake_tx.clone()))))
        .collect();

    let mut cancelled = Box::pin(assets.load::<Decoded>("./shared"));

    let mut first = Box::pin(assets.load::<Decoded>("shared"));

    let clone = assets.clone();

    let mut second = Box::pin(clone.load::<Decoded>("shared"));

    assert!(poll(cancelled.as_mut(), &wakers[0]).is_pending());

    assert_eq!(started.recv_timeout(TIMEOUT).unwrap(), Path::new("shared"));

    assert!(poll(first.as_mut(), &wakers[1]).is_pending());

    assert!(poll(second.as_mut(), &wakers[2]).is_pending());

    assert!(poll(first.as_mut(), &wakers[1]).is_pending());

    drop(cancelled);

    release.send(b"value".to_vec()).unwrap();

    for _ in 0..3 {
        wakes.recv_timeout(TIMEOUT).unwrap();
    }

    assert!(matches!(wakes.try_recv(), Err(mpsc::TryRecvError::Empty)));

    let first_handle = ready(first.as_mut(), &wakers[1]).unwrap();

    let second_handle = ready(second.as_mut(), &wakers[2]).unwrap();

    assert_eq!(first_handle, second_handle);

    assert_eq!(first_handle.0, b"value");

    assert_eq!(reads.load(Ordering::SeqCst), 1);

    assert_eq!(DECODES.load(Ordering::SeqCst), 1);

    let weak = first_handle.downgrade();

    drop(first);

    drop(second);

    drop(first_handle);

    assert!(weak.upgrade().is_some());

    let mut cached = Box::pin(assets.load::<Decoded>("shared"));

    let cached_handle = ready(cached.as_mut(), &wakers[0]).unwrap();

    assert_eq!(cached_handle, second_handle);

    drop(cached);

    drop(cached_handle);

    drop(second_handle);

    assert!(weak.upgrade().is_none());

    let mut reloaded = Box::pin(assets.load::<Decoded>("shared"));

    assert!(poll(reloaded.as_mut(), &wakers[0]).is_pending());

    assert_eq!(started.recv_timeout(TIMEOUT).unwrap(), Path::new("shared"));

    release.send(b"replacement".to_vec()).unwrap();

    wakes.recv_timeout(TIMEOUT).unwrap();

    let reloaded_handle = ready(reloaded.as_mut(), &wakers[0]).unwrap();

    assert_eq!(reloaded_handle.0, b"replacement");

    assert_eq!(DECODES.load(Ordering::SeqCst), 2);

    let mut other_type = Box::pin(assets.load::<Vec<u8>>("shared"));

    assert!(poll(other_type.as_mut(), &wakers[0]).is_pending());

    assert_eq!(started.recv_timeout(TIMEOUT).unwrap(), Path::new("shared"));

    release.send(b"raw".to_vec()).unwrap();

    wakes.recv_timeout(TIMEOUT).unwrap();

    assert_eq!(
        ready(other_type.as_mut(), &wakers[0]).unwrap().as_ref(),
        b"raw"
    );

    assert_eq!(reads.load(Ordering::SeqCst), 3);
}

// Makes resource loading fail both with a normal error and with an unexpected failure.
// Everyone waiting for the resource must be told what happened, and the same loading
// worker must still handle the next healthy request. Invalid file paths must be rejected
// before any attempt to read them.
#[test]
fn loader_failures_notify_all_waiters_and_do_not_kill_the_worker() {
    #[derive(Debug)]
    struct Fallible;

    impl Asset for Fallible {
        fn decode(bytes: Vec<u8>) -> anyhow::Result<Self> {
            assert_ne!(bytes, b"panic", "intentional decoder failure");

            anyhow::ensure!(bytes != b"error", "invalid asset data");

            Ok(Self)
        }
    }

    let (started_tx, started) = mpsc::channel();

    let (release, release_rx) = mpsc::channel();

    let assets = Assets::new(AssetSettings::default().with_workers(1).with_source(
        ControlledSource {
            started: started_tx,
            release: Mutex::new(release_rx),
            reads: Arc::new(AtomicUsize::new(0)),
        },
    ))
    .unwrap();

    let (wake_tx, wakes) = mpsc::channel();

    let first_waker = Waker::from(Arc::new(Notify(wake_tx.clone())));

    let second_waker = Waker::from(Arc::new(Notify(wake_tx)));

    for bytes in [b"error".as_slice(), b"panic", b"healthy"] {
        let mut first = Box::pin(assets.load::<Fallible>("asset"));

        let mut second = Box::pin(assets.load::<Fallible>("asset"));

        assert!(poll(first.as_mut(), &first_waker).is_pending());

        started.recv_timeout(TIMEOUT).unwrap();

        assert!(poll(second.as_mut(), &second_waker).is_pending());

        release.send(bytes.to_vec()).unwrap();

        wakes.recv_timeout(TIMEOUT).unwrap();

        wakes.recv_timeout(TIMEOUT).unwrap();

        for result in [
            ready(first.as_mut(), &first_waker),
            ready(second.as_mut(), &second_waker),
        ] {
            match bytes {
                b"error" => assert!(
                    matches!(result, Err(AssetError::Decode { path, message }) if path == Path::new("asset") && message.contains("invalid asset data"))
                ),
                b"panic" => assert!(
                    matches!(result, Err(AssetError::LoaderPanicked(path)) if path == Path::new("asset"))
                ),
                _ => assert!(result.is_ok()),
            }
        }
    }

    for path in ["../asset", "/asset", "", "./"] {
        let mut invalid = Box::pin(assets.load::<Fallible>(path));

        assert!(matches!(
            ready(invalid.as_mut(), &first_waker),
            Err(AssetError::InvalidPath(_))
        ));
    }

    assert!(matches!(started.try_recv(), Err(mpsc::TryRecvError::Empty)));
}
