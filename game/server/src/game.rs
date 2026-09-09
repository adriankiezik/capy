use capy_engine_protocol::message::PeerId;
use capy_engine_server::{
    Vec2, Vec3,
    character::{Character, CharacterConfig, CharacterPose, MovementInput},
    session::{Context, Game},
};
use capy_protocol::{Command, GAME_ID, PlayerState, State};
use std::collections::BTreeMap;

struct ConnectedPlayer {
    player: Character,
    input: Command,
    last_input: u64,
    last_destroy: u64,
}

#[derive(Default)]
pub struct CapyGame {
    players: BTreeMap<PeerId, ConnectedPlayer>,
}

fn spawn(index: usize) -> anyhow::Result<Character> {
    Ok(Character::new(
        CharacterConfig::default(),
        CharacterPose {
            position: Vec3::new(
                (index % 8) as f32 * 0.8,
                0.01,
                6.0 + (index / 8 % 4) as f32 * 0.8,
            ),
            yaw_radians: 0.0,
            pitch_radians: 0.0,
        },
    )?)
}

impl Game for CapyGame {
    type Command = Command;

    type State = State;

    const ID: &'static str = GAME_ID;

    fn joined(&mut self, peer: PeerId, _: &mut Context<'_>) -> anyhow::Result<()> {
        self.players.insert(
            peer,
            ConnectedPlayer {
                player: spawn(self.players.len())?,
                input: Command::default(),
                last_input: 0,
                last_destroy: 0,
            },
        );

        Ok(())
    }

    fn disconnected(&mut self, peer: PeerId) {
        self.players.remove(&peer);
    }

    fn command(
        &mut self,
        peer: PeerId,
        command: Command,
        context: &mut Context<'_>,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            command
                .movement
                .iter()
                .all(|v| v.is_finite() && v.abs() <= 1.0)
                && command.yaw.is_finite()
                && command.pitch.is_finite()
                && command.yaw.abs() <= 1000.0
                && command.pitch.abs() <= 1.55,
            "invalid player command"
        );

        let player = self
            .players
            .get_mut(&peer)
            .ok_or_else(|| anyhow::anyhow!("unknown player"))?;

        let jump = player.input.jump || command.jump;

        player.input = command;
        player.input.jump = jump;
        player.last_input = context.tick;

        Ok(())
    }

    fn before_step(&mut self, _: &mut Context<'_>) -> anyhow::Result<()> {
        Ok(())
    }

    fn after_step(&mut self, context: &mut Context<'_>) -> anyhow::Result<()> {
        for player in self.players.values_mut() {
            if context.tick.saturating_sub(player.last_input) as f64 * context.delta.as_secs_f64()
                > 0.25
            {
                player.input.movement = [0.0; 2];
                player.input.jump = false;
                player.input.destroy = false;
            }

            let pose = player.player.pose();

            player.player.rotate_view(
                player.input.yaw - pose.yaw_radians,
                player.input.pitch - pose.pitch_radians,
            )?;

            player.player.advance(
                context.simulation,
                MovementInput {
                    movement: Vec2::from_array(player.input.movement),
                    sprint: player.input.sprint,
                    jump: player.input.jump,
                },
                context.delta,
            )?;

            player.input.jump = false;

            if player.player.position().y < -6.0 {
                player.player = spawn(0)?;
            }

            if player.input.destroy
                && context.tick.saturating_sub(player.last_destroy) as f64
                    * context.delta.as_secs_f64()
                    >= 0.1
            {
                if let Some(hit) = context.simulation.raycast(
                    player.player.eye(),
                    player.player.direction(),
                    8.0,
                )? {
                    let _ = context.edits.queue_remove(hit.target);
                }

                player.last_destroy = context.tick;
            }
        }

        Ok(())
    }

    fn state(&self, _: PeerId) -> State {
        State {
            players: self
                .players
                .iter()
                .map(|(&peer, p)| {
                    let pose = p.player.pose();

                    PlayerState {
                        peer,
                        position: pose.position.to_array(),
                        eye: p.player.eye().to_array(),
                        yaw: pose.yaw_radians,
                        pitch: pose.pitch_radians,
                    }
                })
                .collect(),
        }
    }
}
