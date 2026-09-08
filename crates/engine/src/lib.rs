#[cfg(feature = "gpu-bench")]
pub use render::benchmark_gpu;

#[cfg(feature = "scenario-bench")]
pub use render::{ScenarioError, ScenarioRenderer, ScenarioTimings};

pub mod assets;
mod geometry;
mod graphics;
pub mod player;
mod render;
pub mod runtime;
pub mod scene;
pub mod ui;
pub mod world;

pub use assets::{AssetSettings, Assets, Handle};
pub use geometry::Aabb;
pub use glam::{IVec3, Vec2, Vec3};
pub use graphics::{GraphicsSettings, PowerPreference, PresentationMode};
pub use runtime::{
    ApplicationResult, Engine, Frame, RuntimeError, RuntimeSettings, Settings, Tick, View,
    WindowSettings, run,
};
