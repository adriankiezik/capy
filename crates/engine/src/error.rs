use std::error::Error;

use bevy_ecs::error::BevyError;
use thiserror::Error as ThisError;

#[derive(Debug, ThisError)]
pub enum EngineError {
    #[error("{0}")]
    System(BevyError),

    #[error("{0}")]
    Runner(Box<dyn Error + Send + Sync>),
}

impl From<BevyError> for EngineError {
    fn from(e: BevyError) -> Self {
        Self::System(e)
    }
}

pub type Result<T> = std::result::Result<T, EngineError>;
