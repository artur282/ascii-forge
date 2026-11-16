//! Text and FIGlet rendering.
//!
//! [`render_text`] lays out normalized text with greedy word wrapping.
//! [`render_figlet`] renders text with a FIGlet font, falls back to smaller
//! built-in fonts when the result is too large, and finally downscales the
//! rendered block with nearest-neighbor sampling so it fits the canvas.

use std::path::Path;

use figlet_rs::FIGlet;
use thiserror::Error;
use unicode_normalization::UnicodeNormalization;

use crate::scene::{Align, Cell, Grid, Rgba};

/// Fully transparent cell background used by rendered layers.
const TRANSPARENT: Rgba = Rgba::new(0, 0, 0, 0);

/// Catalog of fonts bundled with `figlet-rs`.
const BUILT_IN_FONTS: [&str; 4] = ["standard", "small", "big", "slant"];

/// Fonts tried after the requested font, smallest first.
const FALLBACK_FONTS: [&str; 4] = ["small", "standard", "big", "slant"];

#[derive(Debug, Error)]
pub enum FigletError {
    #[error("unknown FIGlet font: {0}")]
    UnknownFont(String),
    #[error("FIGlet rendering failed: {0}")]
    Render(String),
    #[error("text does not fit in {cols}x{rows} cells")]
    DoesNotFit { cols: u16, rows: u16 },
}

#[derive(Debug, Clone)]
pub struct FigletOptions {
    pub font: String,
    pub color: Option<Rgba>,
    pub max_cols: u16,
    pub max_rows: u16,
    pub align: Align,
}

impl Default for FigletOptions {
    fn default() -> Self {
        Self {
            font: "standard".to_string(),
            color: None,
            max_cols: 80,
            max_rows: 24,
            align: Align::Center,
        }
    }
}

/// Built-in FIGlet fonts accepted by [`FigletOptions::font`].
pub fn font_names() -> &'static [&'static str] {
    &BUILT_IN_FONTS
}

/// Normalize text so every cell maps to a printable ASCII glyph.
///
/// The text is NFKD-decomposed and combining marks are dropped, so accented
/// letters lose their accents. Typographic quotes and dashes are mapped to
/// their ASCII equivalents, and any other non-ASCII character becomes `?`.
/// Control whitespace becomes a space. Input without printable content
/// normalizes to an empty string.
pub fn normalize_text(text: &str) -> String {
    let mut normalized = String::with_capacity(text.len());
    for ch in text.nfkd() {
        if unicode_normalization::char::is_combining_mark(ch) {
            continue;
        }
        normalized.push(ascii_glyph(ch));
    }
    if normalized.chars().all(char::is_whitespace) {
        return String::new();
    }
    normalized
}

/// Render plain text into a grid that is `max_cols` wide and content-tall.
///
/// Words are wrapped at `max_cols`; words longer than the canvas are hard
/// broken. Each row is padded and placed according to the requested alignment.
/// Text that needs more than `max_rows` rows returns [`FigletError::DoesNotFit`].
/// The `font` option is ignored: plain text uses the renderer font, not a
/// FIGlet font.
pub fn render_text(text: &str, options: &FigletOptions) -> Result<Grid, FigletError> {
    let normalized = normalize_text(text);
    let foreground = options.color.unwrap_or_default();
    if normalized.is_empty() {
        return Ok(empty_grid(foreground));
    }
    if options.max_cols == 0 || options.max_rows == 0 {
        return Err(does_not_fit(options));
    }

    let lines = wrap_text(&normalized, usize::from(options.max_cols));
    if lines.len() > usize::from(options.max_rows) {
        return Err(does_not_fit(options));
    }

    let mut grid = Grid::blank(
        options.max_cols,
        lines.len() as u16,
        foreground,
        TRANSPARENT,
    );
    for (y, line) in lines.iter().enumerate() {
        let width = line.chars().count() as u16;
        let offset = align_offset(options.max_cols, width, options.align);
        for (x, ch) in line.chars().enumerate() {
            let cell = Cell {
                ch,
                fg: foreground,
                bg: TRANSPARENT,
            };
            grid.set(offset + x as u16, y as u16, cell);
        }
    }
    Ok(grid)
}

/// Render text with a FIGlet font into a grid that is `max_cols` wide.
///
/// When the rendered block is larger than the canvas it tries, in order, the
/// requested font, `small`, `standard`, `big` and `slant`. If none fit, the
/// block from the requested font is downscaled with nearest-neighbor sampling.
/// The result is padded to `max_cols` and aligned. A zero-sized canvas returns
/// [`FigletError::DoesNotFit`].
pub fn render_figlet(text: &str, options: &FigletOptions) -> Result<Grid, FigletError> {
    let normalized = normalize_text(text);
    let foreground = options.color.unwrap_or_default();
    if normalized.is_empty() {
        return Ok(empty_grid(foreground));
    }
    if options.max_cols == 0 || options.max_rows == 0 {
        return Err(does_not_fit(options));
    }

    let max_cols = usize::from(options.max_cols);
    let max_rows = usize::from(options.max_rows);
    let mut first_block: Option<Vec<String>> = None;

    for name in font_candidates(&options.font) {
        let font = load_font(name)?;
        let lines = figlet_block(&normalized, name, &font)?;
        let (cols, rows) = block_dimensions(&lines);
        if cols == 0 || rows == 0 {
            return Ok(empty_grid(foreground));
        }
        if cols <= max_cols && rows <= max_rows {
            let block = block_grid(&lines, cols as u16, foreground);
            return Ok(layout_grid(
                &block,
                options.max_cols,
                options.align,
                foreground,
            ));
        }
        first_block.get_or_insert(lines);
    }

    let lines = first_block.expect("non-empty text produced a FIGlet block");
    let (cols, rows) = block_dimensions(&lines);
    let (target_cols, target_rows) = scaled_dimensions(cols, rows, max_cols, max_rows);
    let block = sample_block(&lines, cols, rows, target_cols, target_rows, foreground);
    let grid = layout_grid(&block, options.max_cols, options.align, foreground);
    if grid.cols > options.max_cols || grid.rows > options.max_rows {
        return Err(does_not_fit(options));
    }
    Ok(grid)
}

fn does_not_fit(options: &FigletOptions) -> FigletError {
    FigletError::DoesNotFit {
        cols: options.max_cols,
        rows: options.max_rows,
    }
}

fn empty_grid(foreground: Rgba) -> Grid {
    Grid::new(0, 0, blank_cell(foreground))
}

fn blank_cell(foreground: Rgba) -> Cell {
    Cell {
        ch: ' ',
        fg: foreground,
        bg: TRANSPARENT,
    }
}

/// Map one NFKD code point to a printable ASCII glyph.
fn ascii_glyph(ch: char) -> char {
    match ch {
        '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}' => '\'',
        '\u{201C}' | '\u{201D}' | '\u{201E}' | '\u{201F}' => '"',
        '\u{2010}' | '\u{2011}' | '\u{2012}' | '\u{2013}' | '\u{2014}' | '\u{2015}'
        | '\u{2212}' => '-',
        ch if ch == ' ' || ch.is_ascii_graphic() => ch,
        ch if ch.is_ascii() => ' ',
        _ => '?',
    }
}

fn wrap_text(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();

    for word in text.split_whitespace() {
        let mut piece = word;
        loop {
            let used = current.chars().count() + usize::from(!current.is_empty());
            let room = width.saturating_sub(used);
            if piece.chars().count() <= room {
                if !current.is_empty() {
                    current.push(' ');
                }
                current.push_str(piece);
                break;
            }
            if current.is_empty() {
                let (head, tail) = split_at_chars(piece, width);
                lines.push(head.to_string());
                piece = tail;
                if piece.is_empty() {
                    break;
                }
            } else {
                lines.push(std::mem::take(&mut current));
            }
        }
    }

    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

fn split_at_chars(text: &str, count: usize) -> (&str, &str) {
    match text.char_indices().nth(count) {
        Some((index, _)) => text.split_at(index),
        None => (text, ""),
    }
}

fn font_candidates(requested: &str) -> Vec<&str> {
    let mut candidates = Vec::with_capacity(FALLBACK_FONTS.len() + 1);
    for name in std::iter::once(requested).chain(FALLBACK_FONTS) {
        if !candidates.contains(&name) {
            candidates.push(name);
        }
    }
    candidates
}

fn load_font(name: &str) -> Result<FIGlet, FigletError> {
    let built_in = match name {
        "standard" => Some(FIGlet::standard()),
        "small" => Some(FIGlet::small()),
        "big" => Some(FIGlet::big()),
        "slant" => Some(FIGlet::slant()),
        _ => None,
    };
    if let Some(font) = built_in {
        return font.map_err(FigletError::Render);
    }
    if Path::new(name).is_file() || name.to_ascii_lowercase().ends_with(".flf") {
        return FIGlet::from_file(name).map_err(|_| FigletError::UnknownFont(name.to_string()));
    }
    Err(FigletError::UnknownFont(name.to_string()))
}

fn figlet_block(text: &str, font_name: &str, font: &FIGlet) -> Result<Vec<String>, FigletError> {
    let figure = font
        .convert(text)
        .ok_or_else(|| FigletError::Render(format!("font '{font_name}' cannot render the text")))?;
    let rendered = figure.as_str();
    let mut lines: Vec<String> = rendered.split('\n').map(str::to_string).collect();
    while lines.last().is_some_and(|line| line.trim_end().is_empty()) {
        lines.pop();
    }
    Ok(lines)
}

fn block_dimensions(lines: &[String]) -> (usize, usize) {
    let cols = lines
        .iter()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(0);
    (cols, lines.len())
}

fn block_grid(lines: &[String], cols: u16, foreground: Rgba) -> Grid {
    let rows = lines.len() as u16;
    let mut grid = Grid::blank(cols, rows, foreground, TRANSPARENT);
    for (y, line) in lines.iter().enumerate() {
        for (x, ch) in line.chars().enumerate() {
            let cell = Cell {
                ch,
                fg: foreground,
                bg: TRANSPARENT,
            };
            grid.set(x as u16, y as u16, cell);
        }
    }
    grid
}

fn layout_grid(block: &Grid, max_cols: u16, align: Align, foreground: Rgba) -> Grid {
    let mut grid = Grid::blank(max_cols, block.rows, foreground, TRANSPARENT);
    let offset = align_offset(max_cols, block.cols, align);
    for y in 0..block.rows {
        for x in 0..block.cols {
            if let Some(cell) = block.get(x, y) {
                grid.set(x + offset, y, *cell);
            }
        }
    }
    grid
}

fn align_offset(max_cols: u16, width: u16, align: Align) -> u16 {
    let slack = max_cols.saturating_sub(width);
    match align {
        Align::Left => 0,
        Align::Center => slack / 2,
        Align::Right => slack,
    }
}

fn scaled_dimensions(cols: usize, rows: usize, max_cols: usize, max_rows: usize) -> (u16, u16) {
    let scale = f64::min(max_cols as f64 / cols as f64, max_rows as f64 / rows as f64);
    let target_cols = (cols as f64 * scale).floor().max(1.0).min(max_cols as f64);
    let target_rows = (rows as f64 * scale).floor().max(1.0).min(max_rows as f64);
    (target_cols as u16, target_rows as u16)
}

fn sample_block(
    lines: &[String],
    cols: usize,
    rows: usize,
    target_cols: u16,
    target_rows: u16,
    foreground: Rgba,
) -> Grid {
    let mut grid = Grid::blank(target_cols, target_rows, foreground, TRANSPARENT);
    for y in 0..target_rows {
        let source_y = sample_index(y, target_rows, rows);
        for x in 0..target_cols {
            let source_x = sample_index(x, target_cols, cols);
            let ch = line_char(lines, source_y, source_x);
            let cell = Cell {
                ch,
                fg: foreground,
                bg: TRANSPARENT,
            };
            grid.set(x, y, cell);
        }
    }
    grid
}

fn sample_index(target: u16, target_len: u16, source_len: usize) -> usize {
    let source = usize::from(target) * source_len / usize::from(target_len);
    source.min(source_len - 1)
}

fn line_char(lines: &[String], y: usize, x: usize) -> char {
    lines
        .get(y)
        .and_then(|line| line.chars().nth(x))
        .unwrap_or(' ')
}
