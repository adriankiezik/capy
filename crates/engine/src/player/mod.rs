mod config;
mod controller;
mod error;

pub use config::PlayerSettings;
pub use controller::{Camera, Player, PlayerInput, PlayerPose};
pub use error::{PlayerError, Result};

#[cfg(test)]
mod tests;
