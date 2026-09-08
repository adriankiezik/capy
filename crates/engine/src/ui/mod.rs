#[cfg(feature = "cpu-bench")]
mod bench;

mod bitmap;
mod builder;
mod canvas;
mod commands;
mod config;
mod error;
mod font;
mod geometry;
mod layout;
mod paint;
mod raster;

pub use builder::Layout;
pub use canvas::Canvas;
pub use config::{Align, Anchor, Color, Insets, Length, TextStyle};
pub use error::UiError;
pub use font::Font;
pub use geometry::Rect;
pub(crate) use geometry::Vertex;
pub(crate) use raster::GlyphCache;

#[cfg(test)]
mod tests;
