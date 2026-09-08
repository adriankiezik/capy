use super::{
    Canvas, Length, TextStyle, UiError,
    commands::{Commands, Kind},
    config::valid_color,
};
use crate::scene::Vertex;
use glam::Vec2;
use taffy::prelude::*;

fn dimension(value: Length) -> Dimension {
    match value {
        Length::Auto => auto(),
        Length::Fill => percent(1.0),
        Length::Pixels(value) => length(value),
        Length::Percent(value) => percent(value),
    }
}

fn build(
    commands: &Commands,
    tree: &mut TaffyTree<usize>,
    index: usize,
    parent_direction: FlexDirection,
    depth: usize,
) -> Result<NodeId, UiError> {
    if depth > 128 {
        return Err(UiError::ResourceLimit);
    }

    let node = &commands.nodes[index];

    let mut style = node.style.clone();

    if depth > 0 && node.anchored {
        style.position = Position::Absolute;
    }

    style.size = Size {
        width: dimension(node.width),
        height: dimension(node.height),
    };
    style.flex_shrink = if matches!(node.kind, Kind::Rect(_)) {
        0.0
    } else {
        1.0
    };

    let main = if parent_direction == FlexDirection::Row {
        node.width
    } else {
        node.height
    };

    if matches!(main, Length::Fill) {
        style.flex_grow = 1.0;
        style.flex_basis = length(0.0);

        if parent_direction == FlexDirection::Row {
            style.size.width = length(0.0);
        } else {
            style.size.height = length(0.0);
        }
    }

    if style.max_size.width == taffy::LengthPercentageAuto::AUTO {
        style.max_size.width = percent(1.0);
    }

    let children = node
        .children
        .iter()
        .map(|&child| build(commands, tree, child, style.flex_direction, depth + 1))
        .collect::<Result<Vec<_>, _>>()?;

    let id = tree.new_with_children(style, &children)?;

    tree.set_node_context(id, Some(index))?;

    Ok(id)
}

pub(super) fn lines(text: &str, style: &TextStyle, width: f32, wrap: bool) -> Vec<Vec<char>> {
    let columns = if wrap && width.is_finite() {
        ((width / (style.size / 7.0) + 1.0 + 0.001) / 6.0)
            .floor()
            .max(1.0) as usize
    } else {
        usize::MAX
    };

    let mut lines = Vec::new();

    for paragraph in text.split('\n') {
        let chars: Vec<char> = paragraph.chars().collect();

        if chars.is_empty() {
            lines.push(Vec::new());

            continue;
        }

        let mut start = 0;

        while start < chars.len() {
            let mut end = start.saturating_add(columns).min(chars.len());

            if end < chars.len()
                && !chars[end].is_whitespace()
                && let Some(space) = chars[start..end].iter().rposition(|c| c.is_whitespace())
                && space > 0
            {
                end = start + space;
            }

            lines.push(chars[start..end].to_vec());

            start = end;

            if start < chars.len() {
                while start < chars.len() && chars[start].is_whitespace() {
                    start += 1;
                }
            }
        }
    }

    lines
}

pub(super) fn line_width(count: usize, style: &TextStyle) -> f32 {
    (count as f32 * 6.0 - 1.0).max(0.0) * style.size / 7.0
}

fn measure(
    text: &str,
    style: &TextStyle,
    wrap: bool,
    known: Size<Option<f32>>,
    available: Size<AvailableSpace>,
) -> Size<f32> {
    let width = known.width.unwrap_or(match available.width {
        AvailableSpace::Definite(width) => width,
        AvailableSpace::MinContent => text
            .split_whitespace()
            .map(|word| line_width(word.chars().count(), style))
            .fold(0.0, f32::max),
        AvailableSpace::MaxContent => f32::INFINITY,
    });

    let lines = lines(text, style, width, wrap);

    Size {
        width: known.width.unwrap_or_else(|| {
            lines
                .iter()
                .map(|line| line_width(line.len(), style))
                .fold(0.0, f32::max)
        }),
        height: known.height.unwrap_or(
            (lines.len().saturating_sub(1) as f32 * style.line_height + 1.0) * style.size,
        ),
    }
}

impl Canvas {
    pub(crate) fn vertices(&self, physical_size: Vec2, dpi: f32) -> Result<Vec<Vertex>, UiError> {
        let commands = self.commands.borrow();

        if commands.invalid
            || !self.scale.is_finite()
            || self.scale <= 0.0
            || !dpi.is_finite()
            || dpi <= 0.0
            || self
                .reference_size
                .is_some_and(|size| !size.is_finite() || size.min_element() <= 0.0)
        {
            return Err(UiError::InvalidLayout);
        }

        if commands.nodes.len() > 8192 {
            return Err(UiError::ResourceLimit);
        }

        for node in &commands.nodes {
            match &node.kind {
                Kind::Text(text, style) => {
                    if text.len() > 65536 {
                        return Err(UiError::ResourceLimit);
                    }

                    if !style.size.is_finite()
                        || style.size <= 0.0
                        || !style.line_height.is_finite()
                        || style.line_height < 1.0
                        || !valid_color(style.color)
                    {
                        return Err(UiError::InvalidLayout);
                    }
                }
                Kind::Rect(color) if !valid_color(*color) => return Err(UiError::InvalidLayout),
                _ => {}
            }
        }

        let logical_size = physical_size / dpi;

        let reference = self.reference_size.unwrap_or(logical_size);

        let fit = if self.reference_size.is_some() {
            (logical_size / reference).min_element()
        } else {
            1.0
        };

        let scale = dpi * self.scale * fit;

        let viewport = reference / self.scale;

        if !scale.is_finite() || scale <= 0.0 || !viewport.is_finite() {
            return Err(UiError::InvalidLayout);
        }

        let origin = (physical_size - viewport * scale) * 0.5;

        let mut tree = TaffyTree::<usize>::new();

        tree.disable_rounding();

        let mut roots = Vec::new();

        for &index in &commands.roots {
            let child = build(&commands, &mut tree, index, FlexDirection::Column, 0)?;

            let anchor = commands.nodes[index].anchor.factors();

            let mut child_style = tree.style(child)?.clone();

            let margin = child_style.margin.map(|value| {
                value
                    .resolve_to_option(viewport.x, |_, _| 0.0)
                    .unwrap_or(0.0)
            });

            let available_width = (viewport.x - margin.left - margin.right).max(0.0);

            child_style.margin = taffy::Rect {
                left: length(0.0),
                right: length(0.0),
                top: length(0.0),
                bottom: length(0.0),
            };
            child_style.max_size.width = length(
                child_style
                    .max_size
                    .width
                    .resolve_to_option(available_width, |_, _| 0.0)
                    .unwrap_or(available_width)
                    .min(available_width),
            );

            tree.set_style(child, child_style)?;

            let root = tree.new_with_children(
                Style {
                    size: Size {
                        width: length(viewport.x),
                        height: length(viewport.y),
                    },
                    padding: margin.map(length),
                    flex_direction: FlexDirection::Column,
                    align_items: Some(if anchor.x == 0.0 {
                        AlignItems::START
                    } else if anchor.x == 0.5 {
                        AlignItems::CENTER
                    } else {
                        AlignItems::END
                    }),
                    justify_content: Some(if anchor.y == 0.0 {
                        JustifyContent::START
                    } else if anchor.y == 0.5 {
                        JustifyContent::CENTER
                    } else {
                        JustifyContent::END
                    }),
                    ..Style::default()
                },
                &[child],
            )?;

            tree.compute_layout_with_measure(
                root,
                Size {
                    width: AvailableSpace::Definite(viewport.x),
                    height: AvailableSpace::Definite(viewport.y),
                },
                |inputs, _, context, style| {
                    taffy::compute_leaf_layout(
                        inputs,
                        style,
                        |_, _| 0.0,
                        |known, available| {
                            if let Some(index) = context
                                && let Kind::Text(text, text_style) = &commands.nodes[*index].kind
                            {
                                measure(
                                    text,
                                    text_style,
                                    commands.nodes[*index].wrap_text,
                                    known,
                                    available,
                                )
                            } else {
                                Size::ZERO
                            }
                        },
                    )
                },
            )?;

            roots.push(child);
        }

        let mut painter = super::paint::Painter::new(origin, scale);

        for root in roots {
            painter.node(
                &commands,
                &tree,
                root,
                (Vec2::ZERO, None),
                super::Rect::new(Vec2::ZERO, viewport),
            )?;
        }

        Ok(painter.finish())
    }
}
