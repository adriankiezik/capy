use anyhow::Context;
use capy_engine::EngineBuilder;
use capy_input::InputPlugin;
use capy_render::RenderPlugin;
use capy_window::WindowPlugin;
use plugins::GamePlugin;

mod plugins;
mod systems;

fn main() -> anyhow::Result<()> {
    EngineBuilder::new()
        .add_plugin(WindowPlugin)
        .add_plugin(InputPlugin)
        .add_plugin(RenderPlugin)
        .add_plugin(GamePlugin)
        .run()
        .context("failed to run game")
}
