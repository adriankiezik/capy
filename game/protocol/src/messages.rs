use capy_engine_protocol::message::PeerId;
use serde::{Deserialize, Serialize};

pub const GAME_ID: &str = "capy/2";

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct Command {
    pub movement: [f32; 2],
    pub yaw: f32,
    pub pitch: f32,
    pub sprint: bool,
    pub jump: bool,
    pub destroy: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlayerState {
    pub peer: PeerId,
    pub position: [f32; 3],
    pub eye: [f32; 3],
    pub yaw: f32,
    pub pitch: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct State {
    pub players: Vec<PlayerState>,
}

impl State {
    pub fn valid(&self, recipient: PeerId) -> bool {
        let mut identities = std::collections::BTreeSet::new();

        self.players.len() <= 4096
            && self.players.iter().all(|player| {
                player.peer.0 != 0
                    && identities.insert(player.peer)
                    && player
                        .position
                        .iter()
                        .chain(&player.eye)
                        .all(|v| v.is_finite() && v.abs() <= 1_000_000.0)
                    && player.yaw.is_finite()
                    && player.pitch.is_finite()
                    && player.pitch.abs() <= 1.55
            })
            && identities.contains(&recipient)
    }
}
