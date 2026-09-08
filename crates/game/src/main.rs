mod controls;
mod game;
mod scene;

use engine::{
    ApplicationResult, GraphicsSettings, PowerPreference, RuntimeSettings, Settings,
    WindowSettings, runtime::LogicalSize,
};

fn main() -> ApplicationResult<()> {
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
        .with_runtime(RuntimeSettings::default().with_tick_rate(20))
        .with_window_controls(controls::window_controls());

    engine::run(settings, game::run)?;

    Ok(())
}
