mod config;
mod controls;
mod game;

use engine::{
    ApplicationResult, GraphicsSettings, PowerPreference, RuntimeSettings, Settings,
    WindowSettings, runtime::LogicalSize,
};

fn main() -> ApplicationResult<()> {
    use clap::Parser;

    let config = config::Config::parse();

    let settings = Settings::default()
        .with_window(
            WindowSettings::default()
                .with_title("Capy")
                .with_inner_size(LogicalSize::new(1440, 900)),
        )
        .with_graphics(GraphicsSettings {
            power_preference: PowerPreference::HighPerformance,
            ..Default::default()
        })
        .with_runtime(RuntimeSettings::default().with_input_rate(20))
        .with_window_controls(controls::window_controls());

    engine::run(settings, move |app| game::run(app, config))?;

    Ok(())
}
