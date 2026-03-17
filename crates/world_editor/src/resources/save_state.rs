use std::time::Instant;

use bevy_ecs::resource::Resource;

#[derive(Resource, Default)]
pub(crate) struct SaveState {
    pub requested: bool,
    pub last_save: Option<(Instant, SaveResult)>,
}

pub(crate) enum SaveResult {
    Success(usize),
    Error(String),
}
