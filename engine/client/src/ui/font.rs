use super::TextStyle;
use crate::assets::Asset;

#[derive(Debug)]
pub struct Font {
    pub(super) data: fontdue::Font,
}

impl Asset for Font {
    fn decode(bytes: Vec<u8>) -> anyhow::Result<Self> {
        let data = fontdue::Font::from_bytes(bytes, fontdue::FontSettings::default())
            .map_err(anyhow::Error::msg)?;

        Ok(Self { data })
    }
}

pub(super) fn advance(style: &TextStyle, c: char) -> f32 {
    style.font.as_ref().map_or(style.size * 6.0 / 7.0, |font| {
        font.data.metrics(c, style.size).advance_width
    })
}

pub(super) fn kern(style: &TextStyle, previous: Option<char>, c: char) -> f32 {
    match (&style.font, previous) {
        (Some(font), Some(previous)) => font
            .data
            .horizontal_kern(previous, c, style.size)
            .unwrap_or(0.0),
        _ => 0.0,
    }
}

pub(super) fn height(style: &TextStyle) -> f32 {
    style
        .font
        .as_ref()
        .and_then(|font| font.data.horizontal_line_metrics(style.size))
        .map_or(style.size, |metrics| {
            (metrics.ascent - metrics.descent).max(style.size)
        })
}

pub(super) fn ascent(style: &TextStyle) -> f32 {
    style
        .font
        .as_ref()
        .and_then(|font| font.data.horizontal_line_metrics(style.size))
        .map_or(style.size, |metrics| metrics.ascent)
}
