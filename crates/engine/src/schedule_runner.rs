use std::cell::RefCell;

use bevy_ecs::error::{BevyError, ErrorContext};
use bevy_ecs::schedule::ScheduleLabel;
use bevy_ecs::world::World;

thread_local! {
    static CAPTURED_ERROR: RefCell<Option<BevyError>> = const { RefCell::new(None) };
}

pub fn capture_error(error: BevyError, _ctx: ErrorContext) {
    CAPTURED_ERROR.with(|e| {
        *e.borrow_mut() = Some(error);
    });
}

fn take_captured_error() -> Option<BevyError> {
    CAPTURED_ERROR.with(|e| e.borrow_mut().take())
}

pub fn run_schedule(
    world: &mut World,
    label: impl ScheduleLabel,
) -> std::result::Result<(), BevyError> {
    let _ = world.try_run_schedule(label);
    if let Some(err) = take_captured_error() {
        return Err(err);
    }
    Ok(())
}
