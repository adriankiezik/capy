use bevy_ecs::world::World;

use crate::runner::run_windowed;

/// Plugin that provides windowed execution via winit.
///
/// Registers a windowed runner so that `EngineBuilder::run()` creates a window
/// and drives the frame loop through winit's event loop. Binaries that want a
/// window just add this plugin via `EngineBuilder::add_plugin`.
pub struct WindowPlugin;

impl capy_core::Plugin for WindowPlugin {
    fn register(&self, world: &mut World) {
        world.insert_resource(capy_engine::Runner(Box::new(|world| {
            run_windowed(world).map_err(|e| Box::new(e) as _)
        })));
    }
}
