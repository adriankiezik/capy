mod game;

use engine::{RuntimeSettings, Settings, graphics::GraphicsSettings, wgpu, winit};

fn main() -> engine::anyhow::Result<()> {
    let mut graphics = GraphicsSettings::default();

    graphics.adapter_mut().power_preference = wgpu::PowerPreference::HighPerformance;

    let settings = Settings::default()
        .with_window(
            winit::window::Window::default_attributes()
                .with_inner_size(winit::dpi::LogicalSize::new(1280, 800)),
        )
        .with_runtime(RuntimeSettings::default().with_exit_key(winit::keyboard::KeyCode::Escape))
        .with_graphics(graphics);

    engine::run(settings, crate::game::Game::new)?;

    Ok(())
}
