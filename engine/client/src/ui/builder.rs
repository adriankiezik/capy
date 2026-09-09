use super::{
    Align, Anchor, Color, Insets, Length, TextStyle,
    commands::{Commands, Kind, Node},
    config::{valid_color, valid_length},
};
use glam::Vec2;
use std::cell::RefCell;
use taffy::prelude::{FlexDirection, Style};

#[derive(Debug)]
pub struct Layout<'a> {
    commands: &'a RefCell<Commands>,
    id: usize,
}

impl<'a> Layout<'a> {
    pub(super) fn add(commands: &'a RefCell<Commands>, parent: Option<usize>, kind: Kind) -> Self {
        let mut list = commands.borrow_mut();

        let id = list.nodes.len();

        list.nodes.push(Node {
            kind,
            children: Vec::new(),
            style: Style {
                flex_direction: FlexDirection::Column,
                align_items: Some(taffy::AlignItems::START),
                min_size: taffy::Size {
                    width: taffy::prelude::length(0.0),
                    height: taffy::prelude::length(0.0),
                },
                ..Style::default()
            },
            anchor: Anchor::TopLeft,
            anchored: false,
            offset: Vec2::ZERO,
            background: None,
            clip: true,
            wrap_text: true,
            width: Length::Auto,
            height: Length::Auto,
            text_align: Align::Start,
        });

        if let Some(parent) = parent {
            list.nodes[parent].children.push(id);
        } else {
            list.roots.push(id);
        }

        Self { commands, id }
    }

    fn change(self, valid: bool, edit: impl FnOnce(&mut Node)) -> Self {
        let mut commands = self.commands.borrow_mut();

        commands.invalid |= !valid;

        edit(&mut commands.nodes[self.id]);

        drop(commands);

        self
    }

    pub fn panel(&self) -> Layout<'a> {
        Self::add(self.commands, Some(self.id), Kind::Container)
    }

    pub fn row(&self) -> Layout<'a> {
        Self::add(self.commands, Some(self.id), Kind::Container)
            .change(true, |n| n.style.flex_direction = FlexDirection::Row)
    }

    pub fn column(&self) -> Layout<'a> {
        Self::add(self.commands, Some(self.id), Kind::Container)
    }

    pub fn text(&self, text: impl Into<String>, style: &TextStyle) -> Layout<'a> {
        Self::add(
            self.commands,
            Some(self.id),
            Kind::Text(text.into(), style.clone()),
        )
    }

    pub fn rect(&self, size: Vec2, color: Color) -> Layout<'a> {
        Self::add(self.commands, Some(self.id), Kind::Rect(color)).size(size)
    }

    pub fn anchor(self, anchor: Anchor) -> Self {
        self.change(true, |n| {
            n.anchor = anchor;
            n.anchored = true;
        })
    }

    pub fn offset(self, offset: Vec2) -> Self {
        self.change(offset.is_finite(), |n| n.offset = offset)
    }

    pub fn margin(self, margin: impl Into<Insets>) -> Self {
        let margin = margin.into();

        self.change(margin.valid(), |n| {
            n.style.margin = taffy::Rect {
                top: taffy::prelude::length(margin.top),
                right: taffy::prelude::length(margin.right),
                bottom: taffy::prelude::length(margin.bottom),
                left: taffy::prelude::length(margin.left),
            }
        })
    }

    pub fn padding(self, padding: impl Into<Insets>) -> Self {
        let padding = padding.into();

        self.change(padding.valid(), |n| {
            n.style.padding = taffy::Rect {
                top: taffy::prelude::length(padding.top),
                right: taffy::prelude::length(padding.right),
                bottom: taffy::prelude::length(padding.bottom),
                left: taffy::prelude::length(padding.left),
            }
        })
    }

    pub fn background(self, color: Color) -> Self {
        self.change(valid_color(color), |n| n.background = Some(color))
    }

    pub fn gap(self, gap: f32) -> Self {
        self.change(valid_length(gap), |n| {
            n.style.gap = taffy::Size {
                width: taffy::prelude::length(gap),
                height: taffy::prelude::length(gap),
            }
        })
    }

    pub fn align(self, align: Align) -> Self {
        self.change(true, |n| {
            n.text_align = align;
            n.style.align_items = Some(match align {
                Align::Start => taffy::AlignItems::START,
                Align::Center => taffy::AlignItems::CENTER,
                Align::End => taffy::AlignItems::END,
                Align::Stretch => taffy::AlignItems::STRETCH,
            });
        })
    }

    pub fn justify(self, align: Align) -> Self {
        self.change(true, |n| {
            n.style.justify_content = Some(match align {
                Align::Start | Align::Stretch => taffy::JustifyContent::START,
                Align::Center => taffy::JustifyContent::CENTER,
                Align::End => taffy::JustifyContent::END,
            })
        })
    }

    pub fn wrap(self, wrap: bool) -> Self {
        self.change(true, |n| {
            n.wrap_text = wrap;
            n.style.flex_wrap = if wrap {
                taffy::FlexWrap::Wrap
            } else {
                taffy::FlexWrap::NoWrap
            };
        })
    }

    pub fn clip(self, clip: bool) -> Self {
        self.change(true, |n| n.clip = clip)
    }

    pub fn width(self, width: impl Into<Length>) -> Self {
        let width = width.into();

        self.change(width.valid(), |n| n.width = width)
    }

    pub fn height(self, height: impl Into<Length>) -> Self {
        let height = height.into();

        self.change(height.valid(), |n| n.height = height)
    }

    pub fn size(self, size: Vec2) -> Self {
        self.width(size.x).height(size.y)
    }

    pub fn min_size(self, size: Vec2) -> Self {
        self.change(valid_length(size.x) && valid_length(size.y), |n| {
            n.style.min_size = taffy::Size {
                width: taffy::prelude::length(size.x),
                height: taffy::prelude::length(size.y),
            }
        })
    }

    pub fn max_width(self, width: f32) -> Self {
        self.change(valid_length(width), |n| {
            n.style.max_size.width = taffy::prelude::length(width)
        })
    }

    pub fn max_height(self, height: f32) -> Self {
        self.change(valid_length(height), |n| {
            n.style.max_size.height = taffy::prelude::length(height)
        })
    }
}
