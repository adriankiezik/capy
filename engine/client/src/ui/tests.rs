#![allow(clippy::unwrap_used)]

use super::*;
use glam::Vec2;

fn bounds(vertices: &[Vertex]) -> (Vec2, Vec2) {
    vertices.iter().fold(
        (Vec2::splat(f32::INFINITY), Vec2::splat(f32::NEG_INFINITY)),
        |(min, max), vertex| {
            let p = Vec2::from_array(vertex.position);

            assert!(p.is_finite());

            (min.min(p), max.max(p))
        },
    )
}

// Places a panel in the center of the screen with a smaller rectangle attached to its
// lower-right corner. Checks that the rectangle is cut off at the panel edge and that
// both elements stay correctly positioned and sized on different screen sizes and pixel
// densities, including when extra space is needed around the intended display area.
#[test]
fn anchored_and_clipped_hud_scales_with_dpi_and_letterboxes_reference_viewport() {
    let mut canvas = Canvas::new();

    canvas.set_reference_size(Some(Vec2::new(200.0, 100.0)));

    let panel = canvas
        .panel()
        .size(Vec2::new(100.0, 60.0))
        .anchor(Anchor::Center)
        .padding(10.0)
        .background([1.0, 0.0, 0.0, 1.0]);

    panel
        .rect(Vec2::new(30.0, 20.0), [0.0, 1.0, 0.0, 1.0])
        .anchor(Anchor::BottomRight)
        .offset(Vec2::new(20.0, 10.0));

    let mut glyphs = GlyphCache::default();

    for (physical, dpi, scale, origin) in [
        (Vec2::new(200.0, 100.0), 1.0, 1.0, Vec2::ZERO),
        (Vec2::new(400.0, 200.0), 2.0, 2.0, Vec2::ZERO),
        (Vec2::new(400.0, 300.0), 2.0, 2.0, Vec2::new(0.0, 50.0)),
        (Vec2::new(100.0, 100.0), 1.0, 0.5, Vec2::new(0.0, 25.0)),
    ] {
        let vertices = canvas.vertices(physical, dpi, &mut glyphs).unwrap();

        assert_eq!(vertices.len(), 12);

        for (quad, color, min, max) in [
            (
                &vertices[..6],
                [1.0, 0.0, 0.0, 1.0],
                Vec2::new(50.0, 20.0),
                Vec2::new(150.0, 80.0),
            ),
            (
                &vertices[6..],
                [0.0, 1.0, 0.0, 1.0],
                Vec2::new(130.0, 60.0),
                Vec2::new(150.0, 80.0),
            ),
        ] {
            assert!(quad.iter().all(|vertex| vertex.color == color));

            let (actual_min, actual_max) = bounds(quad);

            assert!(
                (actual_min - (min * scale + origin)).abs().max_element() < 1e-5,
                "physical {physical}, dpi {dpi}: {actual_min}"
            );

            assert!((actual_max - (max * scale + origin)).abs().max_element() < 1e-5);
        }
    }
}

// Checks that text wrapping in a narrow space looks exactly like the same text with
// line breaks added by hand. The lines must have the expected spacing, and shortening
// the text area must hide the second line without changing the first.
#[test]
fn wrapped_text_matches_explicit_lines_and_clipping_removes_overflow() {
    let style = TextStyle {
        size: 7.0,
        line_height: 2.0,
        ..TextStyle::default()
    };

    let wrapped = Canvas::new();

    wrapped.text("AA AA", Vec2::ZERO, &style).width(11.0);

    let explicit = Canvas::new();

    explicit.text("AA\nAA", Vec2::ZERO, &style).width(11.0);

    let clipped = Canvas::new();

    clipped
        .text("AA AA", Vec2::ZERO, &style)
        .width(11.0)
        .max_height(8.0);

    let single = Canvas::new();

    single.text("AA", Vec2::ZERO, &style).width(11.0);

    let mut glyphs = GlyphCache::default();

    let mut render = |canvas: &Canvas| {
        canvas
            .vertices(Vec2::new(100.0, 100.0), 1.0, &mut glyphs)
            .unwrap()
    };

    let wrapped = render(&wrapped);

    let explicit = render(&explicit);

    let canonical = |vertices: &[Vertex]| {
        vertices
            .iter()
            .map(|v| (v.position, v.color))
            .collect::<Vec<_>>()
    };

    assert!(!wrapped.is_empty());

    assert_eq!(canonical(&wrapped), canonical(&explicit));

    let (_, max) = bounds(&wrapped);

    assert_eq!(max, Vec2::new(11.0, 21.0));

    assert!(
        wrapped
            .iter()
            .all(|v| v.position[1] <= 7.0 || v.position[1] >= 14.0)
    );

    let clipped = render(&clipped);

    assert_eq!(canonical(&clipped), canonical(&render(&single)));

    assert_eq!(wrapped.len(), clipped.len() * 2);
}
