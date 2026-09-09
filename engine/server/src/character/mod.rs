#[cfg(feature = "cpu-bench")]
mod bench;

mod config;
mod controller;
mod error;

pub use config::CharacterConfig;
pub use controller::{Character, CharacterPose, MovementInput};
pub use error::{CharacterError, Result};

#[cfg(test)]
mod tests;
