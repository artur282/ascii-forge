//! Frame renderers. PNG/ANSI live in raster.rs; SVG lives in svg.rs.

pub mod raster;
pub mod svg;

use crate::scene::Rgba;

#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("font error: {0}")]
    Font(String),
    #[error("image error: {0}")]
    Image(String),
    #[error("svg error: {0}")]
    Svg(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone)]
pub struct RenderOptions {
    pub font_path: Option<String>,
    pub font_size: f32,
    pub scale: f32,
    pub background: Option<Rgba>,
    pub foreground: Option<Rgba>,
    pub transparent: bool,
    /// Exact output width in pixels. Applied after rasterization.
    pub target_width: Option<u32>,
    /// Exact output height in pixels. Applied after rasterization.
    pub target_height: Option<u32>,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            font_path: None,
            font_size: 24.0,
            scale: 1.0,
            background: None,
            foreground: None,
            transparent: false,
            target_width: None,
            target_height: None,
        }
    }
}
