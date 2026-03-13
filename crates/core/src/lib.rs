mod resources;
mod schedule;
mod window;

pub use resources::GameWindow;
pub use schedule::{Render, Startup, Update};
pub use window::Window;

// Re-export commonly used bevy_ecs types for ergonomic imports.
// Downstream crates that need other types should add them here.
pub use bevy_ecs::error::{BevyError, DefaultErrorHandler, ErrorContext};
pub use bevy_ecs::schedule::{IntoScheduleConfigs, ScheduleLabel, Schedules};
pub use bevy_ecs::system::{NonSendMut, Res, ScheduleSystem};
pub use bevy_ecs::world::World;
