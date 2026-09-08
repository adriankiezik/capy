use super::{
    Align, Color, Rect, UiError,
    commands::{Commands, Kind},
    font, layout,
};
use crate::scene::Vertex;
use glam::Vec2;
use taffy::{NodeId, TaffyTree};

pub(super) struct Painter {
    vertices: Vec<Vertex>,
    origin: Vec2,
    scale: f32,
}

impl Painter {
    pub(super) fn new(origin: Vec2, scale: f32) -> Self {
        Self {
            vertices: Vec::new(),
            origin,
            scale,
        }
    }

    pub(super) fn finish(self) -> Vec<Vertex> {
        self.vertices
    }

    fn rect(&mut self, rect: Rect, clip: Rect, color: Color) -> Result<(), UiError> {
        let rect = rect.intersection(clip);

        if rect.size.min_element() <= 0.0 || color[3] == 0.0 {
            return Ok(());
        }

        if self.vertices.len() > 1_000_000 - 6 {
            return Err(UiError::ResourceLimit);
        }

        for offset in [
            Vec2::ZERO,
            Vec2::X,
            Vec2::ONE,
            Vec2::ZERO,
            Vec2::ONE,
            Vec2::Y,
        ] {
            let position = self.origin + (rect.position + offset * rect.size) * self.scale;

            if !position.is_finite() {
                return Err(UiError::InvalidLayout);
            }

            self.vertices.push(Vertex {
                position: [position.x, position.y, 0.0],
                normal: [color[3], 0.0, 0.0],
                color: [color[0], color[1], color[2]],
            });
        }

        Ok(())
    }

    pub(super) fn node(
        &mut self,
        commands: &Commands,
        tree: &TaffyTree<usize>,
        id: NodeId,
        parent: (Vec2, Option<Rect>),
        clip: Rect,
    ) -> Result<(), UiError> {
        let Some(&index) = tree.get_node_context(id) else {
            return Ok(());
        };

        let node = &commands.nodes[index];

        let layout = tree.layout(id)?;

        let size = Vec2::new(layout.size.width, layout.size.height);

        let position = if node.anchored
            && let Some(content) = parent.1
        {
            let start = Vec2::new(layout.margin.left, layout.margin.top);

            let end = Vec2::new(layout.margin.right, layout.margin.bottom);

            content.position
                + start
                + (content.size - start - end - size).max(Vec2::ZERO) * node.anchor.factors()
        } else {
            parent.0 + Vec2::new(layout.location.x, layout.location.y)
        } + node.offset;

        let bounds = Rect::new(position, size);

        if !bounds.position.is_finite() || !bounds.size.is_finite() {
            return Err(UiError::InvalidLayout);
        }

        if let Some(color) = node.background {
            self.rect(bounds, clip, color)?;
        }

        let clip = if node.clip {
            clip.intersection(bounds)
        } else {
            clip
        };

        match &node.kind {
            Kind::Rect(color) => self.rect(bounds, clip, *color)?,
            Kind::Text(text, style) => {
                let content_position =
                    position + Vec2::new(layout.padding.left, layout.padding.top);

                let width = (bounds.size.x - layout.padding.left - layout.padding.right).max(0.0);

                let unit = style.size / 7.0;

                for (row, line) in layout::lines(text, style, width, node.wrap_text)
                    .iter()
                    .enumerate()
                {
                    let shift = (width - layout::line_width(line.len(), style)).max(0.0)
                        * match node.text_align {
                            Align::Start | Align::Stretch => 0.0,
                            Align::Center => 0.5,
                            Align::End => 1.0,
                        };

                    let y = content_position.y + row as f32 * style.size * style.line_height;

                    if y >= clip.position.y + clip.size.y {
                        break;
                    }

                    for (column, &c) in line.iter().enumerate() {
                        let x = content_position.x + shift + column as f32 * 6.0 * unit;

                        for (gy, bits) in font::glyph(c).into_iter().enumerate() {
                            for gx in 0..5 {
                                if bits & (1 << (4 - gx)) != 0 {
                                    self.rect(
                                        Rect::new(
                                            Vec2::new(x + gx as f32 * unit, y + gy as f32 * unit),
                                            Vec2::splat(unit),
                                        ),
                                        clip,
                                        style.color,
                                    )?;
                                }
                            }
                        }
                    }
                }
            }
            Kind::Container => {}
        }

        let content = Rect::new(
            position + Vec2::new(layout.padding.left, layout.padding.top),
            (size
                - Vec2::new(
                    layout.padding.left + layout.padding.right,
                    layout.padding.top + layout.padding.bottom,
                ))
            .max(Vec2::ZERO),
        );

        for child in tree.children(id)? {
            self.node(commands, tree, child, (position, Some(content)), clip)?;
        }

        Ok(())
    }
}
