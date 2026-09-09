use super::{WorldDelta, WorldSnapshot};
use serde::{Deserialize, Serialize};

pub const VERSION: u32 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PeerId(pub u64);

#[derive(Debug, Serialize, Deserialize)]
pub enum ClientMessage<C> {
    Hello { version: u32, game: String },
    Command { sequence: u64, command: C },
    Resync,
    Ping,
    Goodbye,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ServerMessage<S> {
    Welcome {
        version: u32,
        tick_rate: u32,
        session: u64,
        peer: PeerId,
        tick: u64,
        revision: u64,
        world: WorldSnapshot,
        game: S,
    },
    Update {
        session: u64,
        tick: u64,
        base: u64,
        revision: u64,
        acknowledged: u64,
        world: WorldDelta,
        game: S,
    },
    Closed {
        reason: String,
    },
}
