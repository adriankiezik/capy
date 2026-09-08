#[cfg(feature = "cpu-bench")]
mod bench;

mod body;
mod config;
mod connectivity;
mod dynamics;
mod edit;
mod error;
mod geometry;
mod halo;
mod query;
mod render;
mod state;
mod support;
mod work;

pub(crate) use body::Body;
pub use config::{EditSettings, SceneSettings, ShadowSettings, SimulationSettings, VisualSettings};
pub use edit::{EditOutcome, SceneEdits};
pub use error::{Result, SceneError};
pub(crate) use geometry::{Mesh, Vertex};
pub use query::{Hit, Target};
pub(crate) use render::{MeshId, MeshInstance};
pub use state::{Scene, SceneStats};

#[cfg(test)]
pub(crate) mod tests;
