use super::{Font, UiError};
use crate::assets::Handle;
use std::{
    collections::HashMap,
    sync::{Arc, Weak},
};

#[derive(Clone, Copy)]
pub(super) struct Span {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub alpha: f32,
}

struct Glyph {
    font: Weak<Font>,
    spans: Arc<[Span]>,
}

#[derive(Default)]
pub(crate) struct GlyphCache {
    glyphs: HashMap<(usize, char, u32), Glyph>,
    bytes: usize,
}

impl GlyphCache {
    pub(super) fn prune(&mut self) {
        self.glyphs.retain(|_, glyph| glyph.font.strong_count() > 0);

        self.bytes = self
            .glyphs
            .values()
            .map(|glyph| std::mem::size_of_val(glyph.spans.as_ref()))
            .sum();
    }

    pub(super) fn glyph(
        &mut self,
        font: &Handle<Font>,
        c: char,
        size: f32,
    ) -> Result<Arc<[Span]>, UiError> {
        if !size.is_finite() || size <= 0.0 {
            return Err(UiError::InvalidLayout);
        }

        let weak = font.downgrade();

        let key = (weak.as_ptr() as usize, c, size.to_bits());

        if let Some(glyph) = self.glyphs.get(&key) {
            return Ok(glyph.spans.clone());
        }

        let metrics = font.data.metrics(c, size);

        if metrics.width > 1_000_000
            || metrics.height > 1_000_000
            || metrics.width.saturating_mul(metrics.height) > 1_000_000
        {
            return Err(UiError::ResourceLimit);
        }

        let (metrics, bitmap) = font.data.rasterize(c, size);

        let mut spans = Vec::new();

        if metrics.width > 0 {
            for (y, row) in bitmap.chunks_exact(metrics.width).enumerate() {
                let mut x = 0;

                while x < row.len() {
                    let alpha = row[x];

                    let start = x;

                    while x < row.len() && row[x] == alpha {
                        x += 1;
                    }

                    if alpha > 0 {
                        spans.push(Span {
                            x: metrics.xmin as f32 + start as f32,
                            y: -(metrics.ymin as f32) - metrics.height as f32 + y as f32,
                            width: (x - start) as f32,
                            alpha: alpha as f32 / 255.0,
                        });
                    }
                }
            }
        }

        let bytes = std::mem::size_of_val(spans.as_slice());

        if self.bytes + bytes > 8 * 1024 * 1024 || self.glyphs.len() >= 4096 {
            self.glyphs.clear();

            self.bytes = 0;
        }

        let spans: Arc<[Span]> = spans.into();

        if bytes <= 8 * 1024 * 1024 {
            self.bytes += bytes;

            self.glyphs.insert(
                key,
                Glyph {
                    font: weak,
                    spans: spans.clone(),
                },
            );
        }

        Ok(spans)
    }
}
