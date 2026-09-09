use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

pub(super) struct Shutdown {
    stopping: Arc<AtomicBool>,
    #[cfg(unix)]
    signals: Vec<signal_hook::SigId>,
}

impl Shutdown {
    pub fn new() -> std::io::Result<Self> {
        let shutdown = Self {
            stopping: Arc::new(AtomicBool::new(false)),
            #[cfg(unix)]
            signals: Vec::new(),
        };

        #[cfg(unix)]
        {
            let mut shutdown = shutdown;

            for signal in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
                shutdown.signals.push(signal_hook::flag::register(
                    signal,
                    shutdown.stopping.clone(),
                )?);
            }

            Ok(shutdown)
        }

        #[cfg(not(unix))]
        Ok(shutdown)
    }

    pub fn requested(&self) -> bool {
        self.stopping.load(Ordering::Acquire)
    }
}

#[cfg(unix)]
impl Drop for Shutdown {
    fn drop(&mut self) {
        for signal in self.signals.drain(..) {
            signal_hook::low_level::unregister(signal);
        }
    }
}
