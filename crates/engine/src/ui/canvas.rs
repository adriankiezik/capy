use super::{
    Color, Layout, Rect, TextStyle,
    commands::{Commands, Kind},
};
use glam::Vec2;
use std::cell::RefCell;

#[derive(Debug)]
pub struct Canvas {
    pub(super) commands: RefCell<Commands>,
    pub(super) scale: f32,
    pub(super) reference_size: Option<Vec2>,
}

impl Default for Canvas {
    fn default() -> Self {
        Self {
            commands: RefCell::default(),
            scale: 1.0,
            reference_size: None,
        }
    }
}

impl Canvas {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_scale(&mut self, scale: f32) {
        self.scale = scale;
    }

    pub fn set_reference_size(&mut self, size: Option<Vec2>) {
        self.reference_size = size;
    }

    pub fn panel(&self) -> Layout<'_> {
        Layout::add(&self.commands, None, Kind::Container)
    }

    pub fn text(&self, text: impl Into<String>, position: Vec2, style: &TextStyle) -> Layout<'_> {
        Layout::add(&self.commands, None, Kind::Text(text.into(), style.clone())).offset(position)
    }

    pub fn rect(&self, bounds: Rect, color: Color) -> Layout<'_> {
        Layout::add(&self.commands, None, Kind::Rect(color))
            .size(bounds.size)
            .offset(bounds.position)
    }
}
