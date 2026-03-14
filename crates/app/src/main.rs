use anyhow::Context;
use capy_engine::EngineBuilder;
use capy_game::GamePlugin;
use capy_input::InputPlugin;
use capy_render::RenderPlugin;

fn main() -> anyhow::Result<()> {
    EngineBuilder::new()
        .add_plugin(InputPlugin)
        .add_plugin(RenderPlugin)
        .add_plugin(GamePlugin)
        .run()
        .context("failed to run engine")
}
