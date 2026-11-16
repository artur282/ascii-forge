//! PNG, JPEG and ANSI rendering of a single frame.
//!
//! Glyphs are drawn with `ab_glyph` metrics and `imageproc` so every cell keeps
//! a fixed monospace pitch: the cell width comes from the advance of `M` and the
//! cell height from the font's ascent, descent and line gap.

use super::{RenderError, RenderOptions};
use crate::scene::{Cell, Frame, Rgba};
use ab_glyph::{Font, FontVec, PxScale, ScaleFont};
use image::codecs::jpeg::JpegEncoder;
use image::{DynamicImage, ImageFormat, RgbaImage};
use imageproc::drawing::draw_text_mut;
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

/// Quality used for JPEG exports.
const JPEG_QUALITY: u8 = 90;

/// Glyph whose advance defines the cell width.
const CELL_WIDTH_GLYPH: char = 'M';

/// System fonts tried when the caller does not provide `font_path`.
const FALLBACK_FONT_PATHS: [&str; 3] = [
    "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
    "/usr/share/fonts/TTF/DejaVuSansMono.ttf",
    "/System/Library/Fonts/Menlo.ttc",
];

/// Reset sequence appended after every ANSI line.
const ANSI_RESET: &str = "\x1b[0m";

/// Pixel geometry of one character cell, plus the scale used to draw glyphs.
struct CellMetrics {
    width: u32,
    height: u32,
    scale: PxScale,
}

/// Rasterize `frame` into an RGBA image.
///
/// Cells are painted background first, glyph second. When `options.transparent`
/// is set the backgrounds are skipped so uncovered pixels keep alpha 0.
pub fn frame_to_rgba(frame: &Frame, options: &RenderOptions) -> Result<RgbaImage, RenderError> {
    let font = load_font(options)?;
    let metrics = cell_metrics(&font, options)?;
    let (width, height) = image_size(frame, &metrics);
    let mut image = if options.transparent {
        RgbaImage::new(width, height)
    } else {
        RgbaImage::from_pixel(
            width,
            height,
            image::Rgba(canvas_background(frame, options).to_array()),
        )
    };

    for (index, cell) in visible_cells(frame) {
        let x = (index % usize::from(frame.grid.cols)) as u32 * metrics.width;
        let y = (index / usize::from(frame.grid.cols)) as u32 * metrics.height;

        if !options.transparent {
            fill_rect(&mut image, x, y, metrics.width, metrics.height, cell.bg);
        }
        if cell.ch != ' ' {
            draw_cell_glyph(&mut image, cell, options, &font, &metrics, x, y);
        }
    }

    if let (Some(width), Some(height)) = (options.target_width, options.target_height)
        && width > 0
        && height > 0
        && (width != image.width() || height != image.height())
    {
        image =
            image::imageops::resize(&image, width, height, image::imageops::FilterType::Triangle);
    }

    Ok(image)
}

/// Rasterize `frame` and save it as a PNG file.
pub fn render_png(frame: &Frame, options: &RenderOptions, path: &Path) -> Result<(), RenderError> {
    let image = frame_to_rgba(frame, options)?;
    let mut writer = create_file(path)?;
    image
        .write_to(&mut writer, ImageFormat::Png)
        .map_err(image_error)?;
    Ok(())
}

/// Rasterize `frame` and save it as a JPEG file with quality [`JPEG_QUALITY`].
///
/// JPEG has no alpha channel, so transparent frames lose their transparency.
pub fn render_jpeg(frame: &Frame, options: &RenderOptions, path: &Path) -> Result<(), RenderError> {
    let image = frame_to_rgba(frame, options)?;
    let writer = create_file(path)?;
    let encoder = JpegEncoder::new_with_quality(writer, JPEG_QUALITY);
    DynamicImage::ImageRgba8(image)
        .to_rgb8()
        .write_with_encoder(encoder)
        .map_err(image_error)?;
    Ok(())
}

/// Render `frame` as truecolor ANSI, or as plain text when `color` is false.
pub fn frame_to_ansi(frame: &Frame, color: bool) -> String {
    if !color {
        return frame_to_text(frame);
    }

    use std::fmt::Write;

    let mut out = String::new();
    for y in 0..frame.grid.rows {
        let mut previous: Option<(Rgba, Rgba)> = None;
        for x in 0..frame.grid.cols {
            if let Some(cell) = frame.grid.get(x, y) {
                let colors = (cell.fg, cell.bg);
                if previous != Some(colors) {
                    write!(
                        out,
                        "\x1b[38;2;{};{};{}m\x1b[48;2;{};{};{}m",
                        cell.fg.r, cell.fg.g, cell.fg.b, cell.bg.r, cell.bg.g, cell.bg.b
                    )
                    .expect("writing to a String cannot fail");
                    previous = Some(colors);
                }
                out.push(cell.ch);
            }
        }
        out.push_str(ANSI_RESET);
        out.push('\n');
    }
    out
}

/// Render `frame` as plain text with one line per row.
pub fn frame_to_text(frame: &Frame) -> String {
    frame.grid.to_text()
}

fn load_font(options: &RenderOptions) -> Result<FontVec, RenderError> {
    match &options.font_path {
        Some(path) => read_font(path)
            .map_err(|error| RenderError::Font(format!("could not load font '{path}': {error}"))),
        None => {
            for path in FALLBACK_FONT_PATHS {
                if let Ok(font) = read_font(path) {
                    return Ok(font);
                }
            }
            Err(RenderError::Font(format!(
                "no usable monospace font found; set font_path or install one of: {}",
                FALLBACK_FONT_PATHS.join(", ")
            )))
        }
    }
}

fn read_font(path: &str) -> Result<FontVec, String> {
    let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
    FontVec::try_from_vec(bytes).map_err(|error| error.to_string())
}

fn cell_metrics(font: &FontVec, options: &RenderOptions) -> Result<CellMetrics, RenderError> {
    let size = options.font_size * options.scale;
    if !size.is_finite() || size <= 0.0 {
        return Err(RenderError::Font(format!(
            "font size {} with scale {} is not a positive length",
            options.font_size, options.scale
        )));
    }

    let scale = PxScale::from(size);
    let scaled = font.as_scaled(scale);
    let width = scaled.h_advance(scaled.glyph_id(CELL_WIDTH_GLYPH)).ceil() as u32;
    let height = (scaled.ascent() - scaled.descent() + scaled.line_gap()).ceil() as u32;
    if width == 0 || height == 0 {
        return Err(RenderError::Font(format!(
            "font metrics produced empty cells at size {size}"
        )));
    }

    Ok(CellMetrics {
        width,
        height,
        scale,
    })
}

fn image_size(frame: &Frame, metrics: &CellMetrics) -> (u32, u32) {
    (
        u32::from(frame.grid.cols) * metrics.width,
        u32::from(frame.grid.rows) * metrics.height,
    )
}

/// The frame's canvas background is the background of its first cell.
fn canvas_background(frame: &Frame, options: &RenderOptions) -> Rgba {
    options
        .background
        .or_else(|| frame.grid.cells.first().map(|cell| cell.bg))
        .unwrap_or_default()
}

/// Cells that fit inside the declared grid, with their row-major index.
fn visible_cells(frame: &Frame) -> impl Iterator<Item = (usize, &Cell)> {
    let capacity = usize::from(frame.grid.cols) * usize::from(frame.grid.rows);
    frame.grid.cells.iter().enumerate().take(capacity)
}

fn fill_rect(image: &mut RgbaImage, x: u32, y: u32, width: u32, height: u32, color: Rgba) {
    for py in y..y + height {
        for px in x..x + width {
            image.put_pixel(px, py, image::Rgba(color.to_array()));
        }
    }
}

fn draw_cell_glyph(
    image: &mut RgbaImage,
    cell: &Cell,
    options: &RenderOptions,
    font: &FontVec,
    metrics: &CellMetrics,
    x: u32,
    y: u32,
) {
    let color = options.foreground.unwrap_or(cell.fg);
    let mut buffer = [0u8; 4];
    let glyph = cell.ch.encode_utf8(&mut buffer);
    draw_text_mut(
        image,
        image::Rgba(color.to_array()),
        x as i32,
        y as i32,
        metrics.scale,
        font,
        glyph,
    );
}

fn create_file(path: &Path) -> Result<BufWriter<File>, RenderError> {
    File::create(path).map(BufWriter::new).map_err(|error| {
        RenderError::Io(std::io::Error::new(
            error.kind(),
            format!("could not create '{}': {error}", path.display()),
        ))
    })
}

fn image_error(error: image::ImageError) -> RenderError {
    RenderError::Image(error.to_string())
}
