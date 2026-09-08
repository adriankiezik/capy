#[cfg(feature = "cpu-bench")]
mod bench;

mod config;
mod dynamics;
mod error;
mod geometry;
mod query;
mod render;
mod state;
mod support;

pub use config::{SceneSettings, SimulationSettings, VisualSettings};
pub(crate) use dynamics::Body;
pub use error::{Result, SceneError};
pub(crate) use geometry::{Mesh, Vertex};
pub use query::{Hit, Target};
pub(crate) use render::{MeshId, MeshInstance};
pub use state::{Scene, SceneStats};

#[cfg(test)]
pub(crate) mod tests;
