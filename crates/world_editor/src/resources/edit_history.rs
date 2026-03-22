use std::time::Instant;

use bevy_ecs::resource::Resource;
use capy_core::MaterialId;
use glam::IVec3;

pub struct BrickChange {
    pub chunk: [i32; 3],
    pub brick: [u32; 3],
    pub old_materials: [MaterialId; 64],
    pub new_materials: [MaterialId; 64],
}

/// Records a selection corner change (create, move, or resize).
/// `None` corners mean no selection existed (for creation/deletion undo).
pub struct SelectionChange {
    pub old_a: Option<IVec3>,
    pub old_b: Option<IVec3>,
    pub new_a: Option<IVec3>,
    pub new_b: Option<IVec3>,
}

pub enum EditAction {
    /// Voxel data changes (place, remove, paint, sculpt, etc.)
    Voxel { changes: Vec<BrickChange> },
    /// Selection position/size change (move or resize).
    Selection(SelectionChange),
    /// Voxel move: voxel changes + selection change in one atomic action.
    VoxelMove {
        changes: Vec<BrickChange>,
        selection: SelectionChange,
    },
}

/// Delay before key-repeat begins, then interval between repeats.
const REPEAT_DELAY_MS: u64 = 400;
const REPEAT_INTERVAL_MS: u64 = 80;

/// Tracks hold-to-repeat state for a single key (undo or redo).
pub struct RepeatState {
    /// When the key was first pressed (None = not held).
    pub held_since: Option<Instant>,
    /// When the last repeat-fire happened.
    pub last_fire: Option<Instant>,
}

impl Default for RepeatState {
    fn default() -> Self {
        Self {
            held_since: None,
            last_fire: None,
        }
    }
}

impl RepeatState {
    /// Call each frame. Returns `true` if the action should fire this frame.
    /// `just_pressed`: key was pressed this frame (edge).
    /// `held`: key is currently held.
    pub fn update(&mut self, just_pressed: bool, held: bool) -> bool {
        if !held {
            self.held_since = None;
            self.last_fire = None;
            return false;
        }

        let now = Instant::now();

        if just_pressed {
            self.held_since = Some(now);
            self.last_fire = Some(now);
            return true;
        }

        let Some(since) = self.held_since else {
            return false;
        };

        let elapsed = now.duration_since(since).as_millis() as u64;
        if elapsed < REPEAT_DELAY_MS {
            return false;
        }

        let last = self.last_fire.unwrap_or(since);
        if now.duration_since(last).as_millis() as u64 >= REPEAT_INTERVAL_MS {
            self.last_fire = Some(now);
            return true;
        }

        false
    }
}

#[derive(Resource, Default)]
pub struct EditHistory {
    pub undo_stack: Vec<EditAction>,
    pub redo_stack: Vec<EditAction>,
    pub undo_repeat: RepeatState,
    pub redo_repeat: RepeatState,
}
