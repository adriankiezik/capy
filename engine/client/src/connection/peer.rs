use super::{Error, Result};
use capy_engine_protocol::{
    codec::{Framed, Limits, Link, decode, encode},
    message::{ClientMessage, ServerMessage, VERSION},
};
use serde::{Serialize, de::DeserializeOwned};
use std::{
    collections::VecDeque,
    net::{SocketAddr, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

struct Incoming<S> {
    messages: VecDeque<(ServerMessage<S>, usize)>,
    bytes: usize,
    error: Option<Error>,
    resync: bool,
}

pub struct Connection<S> {
    outgoing: mpsc::SyncSender<Vec<u8>>,
    outgoing_bytes: Arc<AtomicUsize>,
    incoming: Arc<Mutex<Incoming<S>>>,
    stopped: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    limits: Limits,
    sequence: u64,
}

impl<S: DeserializeOwned + Send + 'static> Connection<S> {
    pub fn from_link(mut link: Box<dyn Link>, game: &str, limits: Limits) -> Result<Self> {
        limits.validate()?;

        link.send(encode(
            &ClientMessage::<()>::Hello {
                version: VERSION,
                game: game.to_owned(),
            },
            limits,
        )?)?;

        let (outgoing, receiver) = mpsc::sync_channel::<Vec<u8>>(limits.max_queued_messages);

        let incoming = Arc::new(Mutex::new(Incoming {
            messages: VecDeque::new(),
            bytes: 0,
            error: None,
            resync: false,
        }));

        let shared = incoming.clone();

        let outgoing_bytes = Arc::new(AtomicUsize::new(0));

        let queued = outgoing_bytes.clone();

        let stopped = Arc::new(AtomicBool::new(false));

        let stop = stopped.clone();

        let worker = std::thread::Builder::new()
            .name("client-connection".into())
            .spawn(move || {
                let result = (|| -> Result<()> {
                    let mut ping = Instant::now();

                    let mut received = Instant::now();

                    let mut synchronized = false;

                    let mut requested = Instant::now() - Duration::from_secs(2);

                    while !stop.load(Ordering::Acquire) {
                        for _ in 0..limits.max_queued_messages {
                            match receiver.try_recv() {
                                Ok(bytes) => {
                                    queued.fetch_sub(bytes.len(), Ordering::AcqRel);

                                    link.send(bytes)?;
                                }
                                Err(mpsc::TryRecvError::Empty) => break,
                                Err(mpsc::TryRecvError::Disconnected) => return Ok(()),
                            }
                        }

                        if synchronized && ping.elapsed() >= Duration::from_secs(1) {
                            link.send(encode(&ClientMessage::<()>::Ping, limits)?)?;

                            ping = Instant::now();
                        }

                        if shared.lock().unwrap_or_else(|e| e.into_inner()).resync
                            && requested.elapsed() >= Duration::from_secs(1)
                        {
                            link.send(encode(&ClientMessage::<()>::Resync, limits)?)?;

                            requested = Instant::now();
                        }

                        link.flush()?;

                        for _ in 0..limits.max_queued_messages {
                            let Some(bytes) = link.receive()? else {
                                break;
                            };

                            let size = bytes.len();

                            let message: ServerMessage<S> = decode(&bytes, limits)?;

                            received = Instant::now();

                            if matches!(&message, ServerMessage::Welcome { version, .. } if *version != VERSION) {
                                return Err(capy_engine_protocol::codec::Error::Closed.into());
                            }

                            if let ServerMessage::Closed { reason } = message {
                                return Err(Error::Closed(reason));
                            }

                            let welcome = matches!(&message, ServerMessage::Welcome { .. });

                            let mut incoming = shared.lock().unwrap_or_else(|e| e.into_inner());

                            if welcome {
                                synchronized = true;
                                incoming.resync = false;

                                incoming.messages.clear();

                                incoming.bytes = 0;
                            }

                            if incoming.resync {
                                continue;
                            }

                            if incoming.messages.len() >= limits.max_queued_messages
                                || size > limits.max_queued_bytes.saturating_sub(incoming.bytes)
                            {
                                incoming.messages.clear();

                                incoming.bytes = 0;
                                incoming.resync = true;
                            } else {
                                incoming.bytes += size;

                                incoming.messages.push_back((message, size));
                            }
                        }

                        if received.elapsed() > Duration::from_secs(30) {
                            return Err(capy_engine_protocol::codec::Error::Closed.into());
                        }

                        std::thread::sleep(Duration::from_millis(2));
                    }

                    Ok(())
                })();

                if let Err(error) = result {
                    shared.lock().unwrap_or_else(|e| e.into_inner()).error = Some(error);
                }
            })?;

        Ok(Self {
            outgoing,
            outgoing_bytes,
            incoming,
            stopped,
            worker: Some(worker),
            limits,
            sequence: 0,
        })
    }

    pub fn connect(address: SocketAddr, game: &str, limits: Limits) -> Result<Self> {
        let stream = TcpStream::connect_timeout(&address, Duration::from_secs(5))?;

        stream.set_nodelay(true)?;

        stream.set_nonblocking(true)?;

        Self::from_link(Box::new(Framed::new(stream, limits)?), game, limits)
    }

    pub fn send<C: Serialize>(&mut self, command: C) -> Result<u64> {
        self.sequence = self.sequence.checked_add(1).ok_or(Error::Sequence)?;

        self.queue(encode(
            &ClientMessage::Command {
                sequence: self.sequence,
                command,
            },
            self.limits,
        )?)?;

        Ok(self.sequence)
    }

    fn queue(&self, bytes: Vec<u8>) -> Result<()> {
        let size = bytes.len();

        self.outgoing_bytes
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                count
                    .checked_add(size)
                    .filter(|&count| count <= self.limits.max_queued_bytes)
            })
            .map_err(|_| capy_engine_protocol::codec::Error::Limit)?;

        self.outgoing.try_send(bytes).map_err(|error| {
            self.outgoing_bytes.fetch_sub(size, Ordering::AcqRel);

            match error {
                mpsc::TrySendError::Full(_) => capy_engine_protocol::codec::Error::Limit.into(),
                mpsc::TrySendError::Disconnected(_) => {
                    capy_engine_protocol::codec::Error::Closed.into()
                }
            }
        })
    }

    pub fn poll(&mut self) -> Result<Option<ServerMessage<S>>> {
        let mut incoming = self.incoming.lock().unwrap_or_else(|e| e.into_inner());

        if let Some((message, size)) = incoming.messages.pop_front() {
            incoming.bytes -= size;

            return Ok(Some(message));
        }

        if let Some(error) = incoming.error.take() {
            return Err(error);
        }

        Ok(None)
    }

    pub fn resync(&mut self) -> Result<()> {
        let mut incoming = self.incoming.lock().unwrap_or_else(|e| e.into_inner());

        incoming.messages.clear();

        incoming.bytes = 0;
        incoming.resync = true;

        Ok(())
    }
}

impl<S> Drop for Connection<S> {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Release);

        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
