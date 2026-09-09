#[cfg(feature = "cpu-bench")]
mod bench;

mod body;
mod config;
mod connectivity;
mod dynamics;
mod edit;
mod error;
mod query;
mod state;
mod support;
mod work;

pub(crate) use body::Body;
pub use config::{PhysicsConfig, SimulationConfig};
pub use edit::{EditOutcome, EditSettings, SimulationEdits};
pub use error::{Result, SimulationError};
pub use query::{Hit, Target};
pub use state::{Simulation, SimulationStats};

#[cfg(test)]
pub(crate) mod tests;
