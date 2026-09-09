use super::{Error, Result, RuntimeConfig, ServerHandle};
use crate::session::{Game, Session};
use capy_engine_protocol::codec::{Framed, Link, local_pair};
use std::{
    net::TcpListener,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

pub fn spawn_local<G: Game>(
    mut session: Session<G>,
    config: RuntimeConfig,
) -> Result<(Box<dyn Link>, ServerHandle)> {
    let (client, server) = local_pair(session.config().wire)?;

    session.connect(server)?;

    Ok((client, spawn(session, config, None)?))
}

pub fn spawn_dedicated<G: Game>(
    session: Session<G>,
    config: RuntimeConfig,
    listener: TcpListener,
) -> Result<ServerHandle> {
    listener.set_nonblocking(true)?;

    spawn(session, config, Some(listener))
}

fn spawn<G: Game>(
    mut session: Session<G>,
    config: RuntimeConfig,
    listener: Option<TcpListener>,
) -> Result<ServerHandle> {
    if config.max_catch_up_ticks == 0 {
        return Err(Error::Configuration);
    }

    let stop = Arc::new(AtomicBool::new(false));

    let stopping = stop.clone();

    let handle = thread::Builder::new()
        .name("server".into())
        .spawn(move || {
            let delta = Duration::from_secs_f64(1.0 / session.config().tick_rate as f64);

            let mut deadline = Instant::now();

            while !stopping.load(Ordering::Acquire) {
                if let Some(listener) = &listener {
                    for _ in 0..session.config().max_peers {
                        match listener.accept() {
                            Ok((stream, _)) => {
                                stream.set_nonblocking(true)?;

                                stream.set_nodelay(true)?;

                                let link = Framed::new(stream, session.config().wire)?;

                                let _ = session.connect(Box::new(link));
                            }
                            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {
                                continue;
                            }
                            Err(error) => return Err(error.into()),
                        }
                    }
                }

                let mut count = 0;

                while Instant::now() >= deadline
                    && count < config.max_catch_up_ticks
                    && !stopping.load(Ordering::Acquire)
                {
                    session.step()?;

                    deadline += delta;
                    count += 1;
                }

                if Instant::now() >= deadline {
                    deadline = Instant::now() + delta;
                }

                thread::park_timeout(deadline.saturating_duration_since(Instant::now()));
            }

            Ok(())
        })?;

    Ok(ServerHandle {
        stop,
        thread: Some(handle),
    })
}
