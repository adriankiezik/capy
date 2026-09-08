use super::{Align, Anchor, Color, Length, TextStyle};
use glam::Vec2;
use taffy::Style;

#[derive(Debug, Default)]
pub(super) struct Commands {
    pub(super) nodes: Vec<Node>,
    pub(super) roots: Vec<usize>,
    pub(super) invalid: bool,
}

#[derive(Debug)]
pub(super) struct Node {
    pub(super) kind: Kind,
    pub(super) children: Vec<usize>,
    pub(super) style: Style,
    pub(super) anchor: Anchor,
    pub(super) anchored: bool,
    pub(super) offset: Vec2,
    pub(super) background: Option<Color>,
    pub(super) clip: bool,
    pub(super) wrap_text: bool,
    pub(super) width: Length,
    pub(super) height: Length,
    pub(super) text_align: Align,
}

#[derive(Clone, Debug)]
pub(super) enum Kind {
    Container,
    Rect(Color),
    Text(String, TextStyle),
}
