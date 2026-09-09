pub mod assets;
pub mod camera;
pub mod connection;
mod graphics;
pub mod input;

pub use replica::MeshError;

mod render;
pub mod replica;
pub mod runtime;
pub mod ui;

pub use assets::{AssetSettings, Assets, Handle};
pub use capy_engine_protocol::Aabb;
pub use glam::{IVec3, Vec2, Vec3};
pub use graphics::{GraphicsSettings, PowerPreference, PresentationMode};
pub use runtime::{
    ApplicationResult, Engine, Frame, InputUpdate, RuntimeError, RuntimeSettings, Settings, View,
    WindowSettings, run,
};
