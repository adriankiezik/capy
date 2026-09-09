mod error;
mod geometry;
mod halo;
mod scheduler;
mod work;

pub use error::MeshError;
pub(crate) use error::Result;
pub(crate) use geometry::{Mesh, MeshSource, MeshSources, Vertex, key};
pub(crate) use scheduler::Scheduler;

#[cfg(feature = "cpu-bench")]
mod bench;
