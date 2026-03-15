use anyhow::Context;
use capy_engine::EngineBuilder;
use capy_input::InputPlugin;
use capy_render::RenderPlugin;

mod plugins;
mod systems;

fn main() -> anyhow::Result<()> {
    EngineBuilder::new()
        .add_core_plugin(InputPlugin)
        .add_core_plugin(RenderPlugin)
        .add_core_plugin(plugins::GamePlugin)
        .run()
        .context("failed to run game")
}
