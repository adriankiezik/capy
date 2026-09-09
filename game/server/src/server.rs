use crate::{CapyGame, world};
use capy_engine_server::runtime::ServerBuilder;

pub fn create_server() -> ServerBuilder<CapyGame> {
    ServerBuilder::new(|| Ok((world::create()?, CapyGame::default())))
}
