use anyhow::Context;
use capy_core::IntoScheduleConfigs;
use capy_engine::EngineBuilder;

fn main() -> anyhow::Result<()> {
    EngineBuilder::new()
        .add_systems(capy_core::Startup, capy_render::init_renderer)
        .add_systems(
            capy_core::Render,
            (capy_render::resize_system, capy_render::render_system).chain(),
        )
        .run()
        .context("failed to run engine")
}
