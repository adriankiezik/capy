use bevy_ecs::world::World;
use winit::event_loop::{ControlFlow, EventLoop};

use crate::app::App;
use crate::error::WindowError;

pub fn run_windowed(world: World) -> Result<(), WindowError> {
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App::new(world);
    event_loop.run_app(&mut app)?;
    if let Some(error) = app.error {
        return Err(error);
    }
    Ok(())
}
