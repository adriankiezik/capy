use anyhow::Context;
use capy_engine::EngineBuilder;
use capy_input::InputPlugin;
use capy_render::RenderPlugin;
use capy_shared::EguiIntegrationPlugin;
use capy_window::WindowPlugin;
use plugins::EditorPlugin;

mod plugins;
mod systems;

fn main() -> anyhow::Result<()> {
    EngineBuilder::new()
        .add_plugin(WindowPlugin)
        .add_plugin(InputPlugin)
        .add_plugin(RenderPlugin)
        .add_plugin(EguiIntegrationPlugin)
        .add_plugin(EditorPlugin)
        .run()
        .context("failed to run world editor")
}
