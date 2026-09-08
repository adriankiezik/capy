use super::Font;
use crate::assets::Handle;
use glam::Vec2;

pub type Color = [f32; 4];

#[derive(Clone, Copy, Debug, Default)]
pub enum Anchor {
    #[default]
    TopLeft,
    TopCenter,
    TopRight,
    CenterLeft,
    Center,
    CenterRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

impl Anchor {
    pub(super) fn factors(self) -> Vec2 {
        match self {
            Self::TopLeft => Vec2::new(0.0, 0.0),
            Self::TopCenter => Vec2::new(0.5, 0.0),
            Self::TopRight => Vec2::new(1.0, 0.0),
            Self::CenterLeft => Vec2::new(0.0, 0.5),
            Self::Center => Vec2::new(0.5, 0.5),
            Self::CenterRight => Vec2::new(1.0, 0.5),
            Self::BottomLeft => Vec2::new(0.0, 1.0),
            Self::BottomCenter => Vec2::new(0.5, 1.0),
            Self::BottomRight => Vec2::new(1.0, 1.0),
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub enum Align {
    #[default]
    Start,
    Center,
    End,
    Stretch,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Insets {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl Insets {
    pub fn all(value: f32) -> Self {
        Self {
            top: value,
            right: value,
            bottom: value,
            left: value,
        }
    }

    pub fn symmetric(horizontal: f32, vertical: f32) -> Self {
        Self {
            top: vertical,
            right: horizontal,
            bottom: vertical,
            left: horizontal,
        }
    }

    pub fn bottom(value: f32) -> Self {
        Self {
            bottom: value,
            ..Self::default()
        }
    }

    pub(super) fn valid(self) -> bool {
        [self.top, self.right, self.bottom, self.left]
            .into_iter()
            .all(valid_length)
    }
}

impl From<f32> for Insets {
    fn from(value: f32) -> Self {
        Self::all(value)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub enum Length {
    #[default]
    Auto,
    Pixels(f32),
    Percent(f32),
    Fill,
}

impl Length {
    pub(super) fn valid(self) -> bool {
        match self {
            Self::Auto | Self::Fill => true,
            Self::Pixels(value) | Self::Percent(value) => valid_length(value),
        }
    }
}

impl From<f32> for Length {
    fn from(value: f32) -> Self {
        Self::Pixels(value)
    }
}

#[derive(Clone, Debug)]
pub struct TextStyle {
    pub font: Option<Handle<Font>>,
    pub size: f32,
    pub color: Color,
    pub line_height: f32,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            font: None,
            size: 14.0,
            color: [1.0; 4],
            line_height: 1.4,
        }
    }
}

pub(super) fn valid_length(value: f32) -> bool {
    value.is_finite() && value >= 0.0
}

pub(super) fn valid_color(color: Color) -> bool {
    color.into_iter().all(|v| (0.0..=1.0).contains(&v))
}
