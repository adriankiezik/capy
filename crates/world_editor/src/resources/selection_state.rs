use bevy_ecs::resource::Resource;
use glam::{IVec3, Vec3};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SelectionPhase {
    #[default]
    None,
    Dragging,
    Selected,
    Resizing,
    Moving,
    Pasting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Face {
    XPos,
    XNeg,
    YPos,
    YNeg,
    ZPos,
    ZNeg,
}

impl Face {
    /// Returns 0 for X, 1 for Y, 2 for Z.
    pub fn axis(self) -> usize {
        match self {
            Face::XPos | Face::XNeg => 0,
            Face::YPos | Face::YNeg => 1,
            Face::ZPos | Face::ZNeg => 2,
        }
    }
}

#[derive(Resource, Default)]
pub struct SelectionState {
    pub phase: SelectionPhase,
    pub corner_a: Option<IVec3>,
    pub corner_b: Option<IVec3>,
    pub resize_face: Option<Face>,
    pub include_air: bool,
    /// Original corners saved when entering Moving, for cancel.
    pub pre_move_a: Option<IVec3>,
    pub pre_move_b: Option<IVec3>,
    /// World-space grab point when move started.
    pub grab_point: Vec3,
    /// Initial drag_along_axis value when resize started (for delta-based resize).
    pub resize_anchor: f32,
    /// Original corner value along the resize axis when resize started.
    pub resize_origin: i32,
}

impl SelectionState {
    pub fn aabb_min(&self) -> Option<IVec3> {
        match (self.corner_a, self.corner_b) {
            (Some(a), Some(b)) => Some(a.min(b)),
            _ => None,
        }
    }

    pub fn aabb_max(&self) -> Option<IVec3> {
        match (self.corner_a, self.corner_b) {
            (Some(a), Some(b)) => Some(a.max(b)),
            _ => None,
        }
    }

    pub fn dimensions(&self) -> Option<IVec3> {
        match (self.aabb_min(), self.aabb_max()) {
            (Some(min), Some(max)) => Some(max - min + IVec3::ONE),
            _ => None,
        }
    }

    pub fn clear(&mut self) {
        self.phase = SelectionPhase::None;
        self.corner_a = None;
        self.corner_b = None;
        self.resize_face = None;
        self.pre_move_a = None;
        self.pre_move_b = None;
        self.resize_anchor = 0.0;
        self.resize_origin = 0;
    }
}
