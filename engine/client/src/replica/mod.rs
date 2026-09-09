mod config;
mod error;
mod instances;
pub(crate) mod mesh;
mod model;
mod object;
mod query;
mod render;
mod state;
pub(crate) mod world;

pub use capy_engine_protocol::world::{Hit, Target};
pub use config::{ReplicaConfig, ShadowSettings, VisualSettings};
pub use error::{ReplicaError, Result};
pub use mesh::MeshError;
pub(crate) use mesh::{Mesh, Vertex};
pub use model::{ModelConfig, ModelError, VoxelInstance, VoxelModel};
pub use object::RenderObject;
pub(crate) use render::{MeshId, MeshInstance};
pub use state::Replica;

#[cfg(test)]
mod tests;
