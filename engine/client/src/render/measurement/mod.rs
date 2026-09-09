mod capture;
mod config;
mod error;
mod report;
mod runner;

pub use config::{CapturePaths, MeasurementConfig};
pub use error::Result;
pub use report::{FrameIndex, MeasurementFrame, MeasurementReport};
pub use runner::measure;

#[cfg(test)]
mod tests;
