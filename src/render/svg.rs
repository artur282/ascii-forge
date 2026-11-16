//! SVG rendering.
//!
//! Each row becomes one `<text>` element; consecutive cells that share a
//! foreground color are merged into `<tspan>` runs so per-cell color survives
//! the export.
//!
//! Cell metrics come from `ab_glyph` (the same font stack used for raster
//! output) by measuring the advance of a monospace glyph and the font's ascent,
//! descent and line gap. When neither `options.font_path` nor a known system
//! monospace font can be loaded, the exporter falls back to `0.6em` wide and
//! `1.2em` tall cells. The emitted document always declares
//! `DejaVu Sans Mono, monospace`, so readers do not depend on the machine that
//! produced the file. `options.foreground` is not applied: each cell keeps its
//! own color so layer colors are preserved.

use super::{RenderError, RenderOptions};
use crate::scene::{Cell, Frame, Rgba};
use ab_glyph::{Font, FontVec, PxScale, ScaleFont};
use quick_xml::escape::escape;
use std::fmt::Write as _;
use std::path::Path;

const FONT_FAMILY: &str = "DejaVu Sans Mono, monospace";
const FALLBACK_CELL_WIDTH_RATIO: f32 = 0.6;
const FALLBACK_CELL_HEIGHT_RATIO: f32 = 1.2;
const FALLBACK_ASCENT_RATIO: f32 = 0.8;
const DEFAULT_BACKGROUND: Rgba = Rgba::rgb(0, 0, 0);
const SYSTEM_MONO_FONTS: &[&str] = &[
    "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
    "/usr/share/fonts/dejavu/DejaVuSansMono.ttf",
    "/usr/share/fonts/TTF/DejaVuSansMono.ttf",
    "/usr/share/fonts/truetype/liberation/LiberationMono-Regular.ttf",
    "/usr/share/fonts/truetype/noto/NotoSansMono-Regular.ttf",
    "/usr/share/fonts/truetype/freefont/FreeMono.ttf",
    "/Library/Fonts/DejaVuSansMono.ttf",
    "/System/Library/Fonts/Menlo.ttc",
    "C:\\Windows\\Fonts\\consola.ttf",
    "C:\\Windows\\Fonts\\cour.ttf",
];

#[derive(Debug, Clone, Copy)]
struct CellMetrics {
    width: f32,
    height: f32,
    ascent: f32,
    /// CSS `font-size` (em size) that reproduces this text height.
    ///
    /// `ab_glyph`'s `PxScale` is the pixel height of text (ascent - descent),
    /// while SVG's `font-size` is the em size, so the two differ by the font's
    /// `units_per_em / height_unscaled` ratio.
    em_size: f32,
}

#[derive(Debug, Clone, PartialEq)]
struct ColorRun {
    start_col: usize,
    end_col: usize,
    color: Rgba,
    text: String,
}

/// Render a frame as a standalone SVG document.
pub fn render_svg(frame: &Frame, options: &RenderOptions) -> Result<String, RenderError> {
    let font_size = options.font_size.max(1.0);
    let scale = normalize_scale(options.scale);
    let metrics = measure_cell_metrics(font_size, options.font_path.as_deref());
    let grid = &frame.grid;
    let logical_width = f32::from(grid.cols) * metrics.width;
    let logical_height = f32::from(grid.rows) * metrics.height;
    let output_width = options
        .target_width
        .map_or(logical_width * scale, |width| width as f32);
    let output_height = options
        .target_height
        .map_or(logical_height * scale, |height| height as f32);

    let mut svg = String::new();
    writeln!(
        svg,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}px\" height=\"{}px\" \
         viewBox=\"0 0 {} {}\" font-family=\"{}\" font-size=\"{}\">",
        format_number(output_width),
        format_number(output_height),
        format_number(logical_width),
        format_number(logical_height),
        escape_attr(FONT_FAMILY),
        format_number(metrics.em_size),
    )
    .expect("writing to a String cannot fail");

    if !options.transparent {
        write_background(
            &mut svg,
            options.background.unwrap_or(DEFAULT_BACKGROUND),
            logical_width,
            logical_height,
        );
    }

    for y in 0..grid.rows {
        let baseline = f32::from(y) * metrics.height + metrics.ascent;
        write!(
            svg,
            "<text xml:space=\"preserve\" x=\"0\" y=\"{}\">",
            format_number(baseline)
        )
        .expect("writing to a String cannot fail");
        if let Some(row) = grid.row(y) {
            write_runs(&mut svg, &collect_runs(row), metrics.width);
        }
        svg.push_str("</text>\n");
    }

    svg.push_str("</svg>\n");
    Ok(svg)
}

/// Render a frame as SVG and write it to `path`.
pub fn write_svg(frame: &Frame, options: &RenderOptions, path: &Path) -> Result<(), RenderError> {
    let svg = render_svg(frame, options)?;
    std::fs::write(path, svg)?;
    Ok(())
}

fn normalize_scale(scale: f32) -> f32 {
    if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    }
}

fn write_background(svg: &mut String, color: Rgba, width: f32, height: f32) {
    write!(
        svg,
        "<rect width=\"{}\" height=\"{}\" fill=\"{}\"",
        format_number(width),
        format_number(height),
        escape_attr(&hex_color(color)),
    )
    .expect("writing to a String cannot fail");
    write_opacity(svg, color);
    svg.push_str("/>\n");
}

/// Split a row into maximal runs of adjacent cells with the same foreground.
/// Fully transparent cells (alpha zero) are dropped; the caller repositions
/// the following run with an `x` offset.
fn collect_runs(cells: &[Cell]) -> Vec<ColorRun> {
    let mut runs: Vec<ColorRun> = Vec::new();
    for (col, cell) in cells.iter().enumerate() {
        if cell.fg.a == 0 {
            continue;
        }
        let extends_run = runs
            .last()
            .is_some_and(|run| run.color == cell.fg && run.end_col == col);
        if extends_run {
            let run = runs.last_mut().expect("checked above");
            run.text.push(cell.ch);
            run.end_col = col + 1;
        } else {
            runs.push(ColorRun {
                start_col: col,
                end_col: col + 1,
                color: cell.fg,
                text: cell.ch.to_string(),
            });
        }
    }
    runs
}

fn write_runs(svg: &mut String, runs: &[ColorRun], cell_width: f32) {
    let mut next_col = 0;
    for run in runs {
        write!(
            svg,
            "<tspan fill=\"{}\"",
            escape_attr(&hex_color(run.color))
        )
        .expect("writing to a String cannot fail");
        if run.start_col != next_col {
            let x = run.start_col as f32 * cell_width;
            write!(svg, " x=\"{}\"", format_number(x)).expect("writing to a String cannot fail");
        }
        write_opacity(svg, run.color);
        write!(svg, ">{}</tspan>", escape(run.text.as_str()))
            .expect("writing to a String cannot fail");
        next_col = run.end_col;
    }
}

fn write_opacity(svg: &mut String, color: Rgba) {
    if color.a < u8::MAX {
        let opacity = f32::from(color.a) / f32::from(u8::MAX);
        write!(svg, " fill-opacity=\"{}\"", format_number(opacity))
            .expect("writing to a String cannot fail");
    }
}

fn hex_color(color: Rgba) -> String {
    format!("#{:02x}{:02x}{:02x}", color.r, color.g, color.b)
}

fn escape_attr(value: &str) -> String {
    escape(value).into_owned()
}

fn format_number(value: f32) -> String {
    if (value - value.round()).abs() < 0.001 {
        format!("{:.0}", value)
    } else {
        format!("{:.2}", value)
    }
}

fn measure_cell_metrics(font_size: f32, font_path: Option<&str>) -> CellMetrics {
    if let Some(metrics) = font_path.and_then(|path| metrics_from_font(path, font_size)) {
        return metrics;
    }
    for path in SYSTEM_MONO_FONTS {
        if let Some(metrics) = metrics_from_font(path, font_size) {
            return metrics;
        }
    }
    fallback_cell_metrics(font_size)
}

fn metrics_from_font(path: &str, font_size: f32) -> Option<CellMetrics> {
    let data = std::fs::read(path).ok()?;
    let font = FontVec::try_from_vec(data).ok()?;
    let height_unscaled = font.height_unscaled();
    if !(height_unscaled.is_finite() && height_unscaled > 0.0) {
        return None;
    }
    let scaled = font.as_scaled(PxScale::from(font_size));
    let width = scaled.h_advance(scaled.glyph_id('M'));
    let ascent = scaled.ascent();
    let height = ascent - scaled.descent() + scaled.line_gap();
    if !(width.is_finite() && height.is_finite()) || width <= 0.0 || height <= 0.0 {
        return None;
    }
    let em_size = match font.units_per_em() {
        Some(units) if units.is_finite() && units > 0.0 => font_size * units / height_unscaled,
        _ => font_size,
    };
    Some(CellMetrics {
        width,
        height,
        ascent,
        em_size,
    })
}

fn fallback_cell_metrics(font_size: f32) -> CellMetrics {
    let height = font_size * FALLBACK_CELL_HEIGHT_RATIO;
    CellMetrics {
        width: font_size * FALLBACK_CELL_WIDTH_RATIO,
        height,
        ascent: height * FALLBACK_ASCENT_RATIO,
        em_size: font_size,
    }
}
