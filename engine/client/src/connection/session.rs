use super::{Connection, Error, Result};
use crate::replica::{Replica, ReplicaConfig, ReplicaError};
use capy_engine_protocol::message::{PeerId, ServerMessage};
use serde::{Serialize, de::DeserializeOwned};
use std::time::Duration;

pub struct Update<S> {
    pub state: S,
    pub peer: PeerId,
    pub reset: bool,
    pub interval: Duration,
}

pub struct ClientSession<S> {
    connection: Connection<S>,
    presentation: ReplicaConfig,
    replica: Option<Replica>,
    identity: Option<u64>,
    peer: PeerId,
    tick: u64,
}

impl<S: DeserializeOwned + Send + 'static> ClientSession<S> {
    pub fn new(connection: Connection<S>, presentation: ReplicaConfig) -> Self {
        Self {
            connection,
            presentation,
            replica: None,
            identity: None,
            peer: PeerId(0),
            tick: 0,
        }
    }

    pub fn replica_mut(&mut self) -> Option<&mut Replica> {
        self.replica.as_mut()
    }

    pub fn send<C: Serialize>(&mut self, command: C) -> Result<u64> {
        self.connection.send(command)
    }

    pub fn poll(&mut self) -> Result<Option<Update<S>>> {
        let Some(message) = self.connection.poll()? else {
            return Ok(None);
        };

        match message {
            ServerMessage::Welcome {
                tick_rate,
                session,
                peer,
                tick,
                revision,
                world,
                game,
                ..
            } => {
                if !(1..=1000).contains(&tick_rate) || peer.0 == 0 || tick != revision {
                    return Err(ReplicaError::Invalid.into());
                }

                let interval = Duration::from_secs_f64(1.0 / tick_rate as f64);

                let mut presentation = self.presentation.clone();

                presentation.update_interval = interval;
                self.replica = Some(Replica::new(presentation, revision, world)?);
                self.identity = Some(session);
                self.peer = peer;
                self.tick = tick;
                self.presentation.update_interval = interval;

                Ok(Some(Update {
                    state: game,
                    peer,
                    reset: true,
                    interval,
                }))
            }
            ServerMessage::Update {
                session,
                tick,
                base,
                revision,
                world,
                game,
                ..
            } => {
                if self.identity != Some(session)
                    || base != self.tick
                    || self.tick.checked_add(1) != Some(tick)
                {
                    self.connection.resync()?;

                    return Ok(None);
                }

                if revision != tick {
                    return Err(ReplicaError::Invalid.into());
                }

                let Some(replica) = self.replica.as_mut() else {
                    self.connection.resync()?;

                    return Ok(None);
                };

                replica.apply(base, revision, world)?;

                let interval = self.presentation.update_interval;

                self.tick = tick;

                Ok(Some(Update {
                    state: game,
                    peer: self.peer,
                    reset: false,
                    interval,
                }))
            }
            ServerMessage::Closed { reason } => Err(Error::Closed(reason)),
        }
    }
}
