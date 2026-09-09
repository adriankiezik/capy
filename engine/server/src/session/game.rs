use crate::simulation::{EditOutcome, Simulation, SimulationEdits};
use capy_engine_protocol::message::PeerId;
use serde::{Serialize, de::DeserializeOwned};
use std::time::Duration;

pub struct Context<'a> {
    pub simulation: &'a mut Simulation,
    pub edits: &'a mut SimulationEdits,
    pub outcomes: &'a [EditOutcome],
    pub tick: u64,
    pub delta: Duration,
}

pub trait Game: Send + 'static {
    type Command: DeserializeOwned + Send + 'static;

    type State: Serialize;

    const ID: &'static str;

    fn joined(&mut self, peer: PeerId, context: &mut Context<'_>) -> anyhow::Result<()>;

    fn disconnected(&mut self, peer: PeerId);

    fn command(
        &mut self,
        peer: PeerId,
        command: Self::Command,
        context: &mut Context<'_>,
    ) -> anyhow::Result<()>;

    fn before_step(&mut self, context: &mut Context<'_>) -> anyhow::Result<()>;

    fn after_step(&mut self, context: &mut Context<'_>) -> anyhow::Result<()>;

    fn state(&self, peer: PeerId) -> Self::State;
}
