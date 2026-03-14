use anyhow::Context;
use capy_engine::EngineBuilder;
use capy_input::InputPlugin;
use capy_render::RenderPlugin;

mod editor_plugin;

use editor_plugin::EditorPlugin;

fn main() -> anyhow::Result<()> {
    EngineBuilder::new()
        .add_plugin(InputPlugin)
        .add_plugin(RenderPlugin)
        .add_plugin(EditorPlugin)
        .run()
        .context("failed to run world editor")
}
