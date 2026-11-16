//! Conversion from a composed scene frame to a ratatui buffer.

use crate::scene::{Cell as SceneCell, Frame, Rgba};
use crate::tui::color::{self, ColorMode};
use ratatui::buffer::{Buffer, Cell};
use ratatui::layout::{Position, Rect};
use ratatui::style::Color;

/// Build a buffer for `area`, copying the top-left region of the frame's grid.
///
/// The copy is clipped to both the grid and the area, so tiny terminals and
/// empty grids are safe. Colors depend on `mode`.
pub fn frame_to_buffer(frame: &Frame, area: Rect, mode: ColorMode) -> Buffer {
    let mut buffer = Buffer::empty(area);
    let cols = frame.grid.cols.min(area.width);
    let rows = frame.grid.rows.min(area.height);

    for y in 0..rows {
        for x in 0..cols {
            let Some(source) = frame.grid.get(x, y) else {
                continue;
            };
            let Some(target) = buffer.cell_mut(Position::new(area.x + x, area.y + y)) else {
                continue;
            };
            *target = to_buffer_cell(source, mode);
        }
    }

    buffer
}

fn to_buffer_cell(source: &SceneCell, mode: ColorMode) -> Cell {
    let mut cell = Cell::default();
    let mut symbol = [0u8; 4];
    cell.set_symbol(source.ch.encode_utf8(&mut symbol));
    let (foreground, background) = colors(source, mode);
    cell.set_fg(foreground).set_bg(background);
    cell
}

fn colors(source: &SceneCell, mode: ColorMode) -> (Color, Color) {
    match mode {
        ColorMode::TrueColor => (rgb(source.fg), rgb(source.bg)),
        ColorMode::Ansi256 => (
            Color::Indexed(color::xterm_index(source.fg)),
            Color::Indexed(color::xterm_index(source.bg)),
        ),
        ColorMode::Ansi16 => (color::ansi16(source.fg), color::ansi16(source.bg)),
        ColorMode::NoColor => (Color::Reset, Color::Reset),
    }
}

fn rgb(color: Rgba) -> Color {
    Color::Rgb(color.r, color.g, color.b)
}
