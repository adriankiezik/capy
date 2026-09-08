#[cfg(feature = "gpu-bench")]
pub use renderer::bench::run as benchmark_gpu;

#[cfg(feature = "scenario-bench")]
mod scenario;

#[cfg(feature = "scenario-bench")]
pub use scenario::{ScenarioRenderer, ScenarioTimings};

mod overlay;
mod renderer;
mod ui;
mod world;

pub(crate) use renderer::Renderer;

#[cfg(test)]
mod tests;
