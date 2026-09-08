#![allow(clippy::expect_used)]

use super::{Canvas, GlyphCache, TextStyle, layout};
use divan::{Bencher, black_box, counter::ItemsCount};
use glam::Vec2;

#[divan::bench(args = [16, 128, 1024])]
fn text_wrap(bencher: Bencher, words: usize) {
    let text = "benchmark text with varied words ".repeat(words / 5 + 1);

    let text = text
        .split_whitespace()
        .take(words)
        .collect::<Vec<_>>()
        .join(" ");

    let style = TextStyle::default();

    bencher
        .counter(ItemsCount::new(words))
        .bench_local(|| layout::lines(black_box(&text), black_box(&style), 320.0, true));
}

#[divan::bench(args = [16, 128, 1024])]
fn layout_and_vertices(bencher: Bencher, nodes: usize) {
    let canvas = Canvas::new();

    let root = canvas.panel().size(Vec2::new(1920.0, 1080.0));

    let grid = root.row().wrap(true).gap(2.0);

    for _ in 0..nodes {
        grid.rect(Vec2::new(20.0, 20.0), [0.4, 0.5, 0.6, 1.0]);
    }

    let mut glyphs = GlyphCache::default();

    bencher.counter(ItemsCount::new(nodes)).bench_local(|| {
        black_box(&canvas)
            .vertices(Vec2::new(1920.0, 1080.0), 1.0, &mut glyphs)
            .expect("valid UI fixture")
    });
}
