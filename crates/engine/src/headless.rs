use std::time::{Duration, Instant};

use bevy_ecs::world::World;

use crate::error::EngineError;
use crate::resources::TickRate;
use crate::schedule_runner;

pub fn run_headless(mut world: World) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    schedule_runner::run_schedule(&mut world, capy_core::PreStartup).map_err(EngineError::from)?;
    schedule_runner::run_schedule(&mut world, capy_core::Startup).map_err(EngineError::from)?;

    let tick_duration = {
        let tps = world
            .get_resource::<TickRate>()
            .map_or(TickRate::default().ticks_per_second, |r| r.ticks_per_second);
        Duration::from_secs_f64(1.0 / f64::from(tps))
    };

    while world.get_resource::<capy_core::AppExit>().is_none() {
        let frame_start = Instant::now();

        schedule_runner::run_schedule(&mut world, capy_core::Update).map_err(EngineError::from)?;
        schedule_runner::run_schedule(&mut world, capy_core::Render).map_err(EngineError::from)?;

        let elapsed = frame_start.elapsed();
        if let Some(remaining) = tick_duration.checked_sub(elapsed) {
            std::thread::sleep(remaining);
        }
    }

    Ok(())
}
