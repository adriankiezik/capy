use super::{Context, Error, Game, Result, SessionConfig};
use crate::{
    replication::Publisher,
    simulation::{Simulation, SimulationEdits},
};
use capy_engine_protocol::{
    codec::{Link, decode, encode},
    message::{ClientMessage, PeerId, ServerMessage, VERSION},
};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

struct Peer {
    link: Box<dyn Link>,
    joined: bool,
    snapshot: bool,
    sequence: u64,
    last_message: Instant,
    last_snapshot: u64,
}

pub struct Session<G: Game> {
    config: SessionConfig,
    simulation: Simulation,
    edits: SimulationEdits,
    game: G,
    peers: BTreeMap<PeerId, Peer>,
    next_peer: u64,
    tick: u64,
    identity: u64,
    publisher: Publisher,
}

impl<G: Game> Session<G> {
    pub fn new(config: SessionConfig, simulation: Simulation, game: G) -> Result<Self> {
        config.validate()?;

        let edits = SimulationEdits::new(&simulation, Default::default())?;

        let mut publisher = Publisher::new();

        publisher.update(&simulation, 0);

        Ok(Self {
            config,
            simulation,
            edits,
            game,
            peers: BTreeMap::new(),
            next_peer: 1,
            tick: 0,
            identity: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos() as u64,
            publisher,
        })
    }

    pub fn config(&self) -> &SessionConfig {
        &self.config
    }

    pub fn connect(&mut self, link: Box<dyn Link>) -> Result<()> {
        if self.peers.len() >= self.config.max_peers {
            return Err(Error::Capacity);
        }

        let id = PeerId(self.next_peer);

        self.next_peer = self.next_peer.checked_add(1).ok_or(Error::Counter)?;

        self.peers.insert(
            id,
            Peer {
                link,
                joined: false,
                snapshot: true,
                sequence: 0,
                last_message: Instant::now(),
                last_snapshot: 0,
            },
        );

        Ok(())
    }

    fn receive(&mut self, id: PeerId, peer: &mut Peer) -> Result<()> {
        if peer.last_message.elapsed() > self.config.connection_timeout {
            return Err(Error::Protocol(capy_engine_protocol::codec::Error::Closed));
        }

        peer.link.flush()?;

        for _ in 0..self.config.max_commands_per_tick {
            let Some(bytes) = peer.link.receive()? else {
                break;
            };

            let message: ClientMessage<G::Command> = decode(&bytes, self.config.wire)?;

            peer.last_message = Instant::now();

            let mut context = Context {
                simulation: &mut self.simulation,
                edits: &mut self.edits,
                outcomes: &[],
                tick: self.tick,
                delta: Duration::from_secs_f64(1.0 / self.config.tick_rate as f64),
            };

            match message {
                ClientMessage::Hello { version, game }
                    if !peer.joined && version == VERSION && game == G::ID =>
                {
                    self.game.joined(id, &mut context)?;

                    peer.joined = true;
                }
                ClientMessage::Command { sequence, command }
                    if peer.joined && sequence > peer.sequence =>
                {
                    self.game.command(id, command, &mut context)?;

                    peer.sequence = sequence;
                }
                ClientMessage::Resync if peer.joined => {
                    if self.tick.saturating_sub(peer.last_snapshot) >= self.config.tick_rate as u64
                    {
                        peer.snapshot = true;
                    }
                }
                ClientMessage::Ping if peer.joined => {}
                _ => return Err(Error::Protocol(capy_engine_protocol::codec::Error::Closed)),
            }
        }

        Ok(())
    }

    pub fn step(&mut self) -> Result<()> {
        let ids: Vec<_> = self.peers.keys().copied().collect();

        for id in ids {
            let Some(mut peer) = self.peers.remove(&id) else {
                continue;
            };

            if self.receive(id, &mut peer).is_ok() {
                self.peers.insert(id, peer);
            } else {
                self.game.disconnected(id);
            }
        }

        let outcomes = self.edits.update(&mut self.simulation)?;

        let delta = Duration::from_secs_f64(1.0 / self.config.tick_rate as f64);

        self.game.before_step(&mut Context {
            simulation: &mut self.simulation,
            edits: &mut self.edits,
            outcomes: &outcomes,
            tick: self.tick,
            delta,
        })?;

        self.simulation.advance(delta)?;

        self.game.after_step(&mut Context {
            simulation: &mut self.simulation,
            edits: &mut self.edits,
            outcomes: &outcomes,
            tick: self.tick,
            delta,
        })?;

        let base = self.tick;

        self.tick = self.tick.checked_add(1).ok_or(Error::Counter)?;

        let changes = self.publisher.update(&self.simulation, self.tick);

        let mut disconnected = Vec::new();

        for (&id, peer) in &mut self.peers {
            if !peer.joined {
                continue;
            }

            let message = if peer.snapshot {
                ServerMessage::Welcome {
                    version: VERSION,
                    tick_rate: self.config.tick_rate,
                    session: self.identity,
                    peer: id,
                    tick: self.tick,
                    revision: self.tick,
                    world: self.publisher.snapshot(&self.simulation),
                    game: self.game.state(id),
                }
            } else {
                ServerMessage::Update {
                    session: self.identity,
                    tick: self.tick,
                    base,
                    revision: self.tick,
                    acknowledged: peer.sequence,
                    world: changes.clone(),
                    game: self.game.state(id),
                }
            };

            if encode(&message, self.config.wire)
                .and_then(|bytes| peer.link.send(bytes))
                .is_err()
            {
                disconnected.push(id);
            } else if peer.snapshot {
                peer.snapshot = false;
                peer.last_snapshot = self.tick;
            }
        }

        for id in disconnected {
            self.peers.remove(&id);

            self.game.disconnected(id);
        }

        Ok(())
    }
}

impl<G: Game> Drop for Session<G> {
    fn drop(&mut self) {
        let message = encode(
            &ServerMessage::<G::State>::Closed {
                reason: "server stopped".into(),
            },
            self.config.wire,
        );

        for (id, mut peer) in std::mem::take(&mut self.peers) {
            if peer.joined {
                if let Ok(bytes) = &message {
                    let _ = peer.link.send(bytes.clone());
                }

                self.game.disconnected(id);
            }
        }
    }
}
