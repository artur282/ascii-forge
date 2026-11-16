//! Tests for the pure preview surface: color modes and buffer conversion.

use ascii_forge::scene::{Cell as SceneCell, Frame, Grid, Rgba};
use ascii_forge::tui::{ColorMode, color_mode_for, detect_color_mode, frame_to_buffer};
use ratatui::buffer::{Buffer, Cell};
use ratatui::layout::Rect;
use ratatui::style::Color;

fn cell(ch: char, fg: Rgba, bg: Rgba) -> SceneCell {
    SceneCell { ch, fg, bg }
}

fn frame_with(cells: &[SceneCell], cols: u16, rows: u16) -> Frame {
    Frame {
        index: 0,
        grid: Grid {
            cols,
            rows,
            cells: cells.to_vec(),
        },
    }
}

fn buffer_cell(buffer: &Buffer, x: u16, y: u16) -> &Cell {
    buffer.cell((x, y)).expect("cell is inside the buffer")
}

#[test]
fn truecolor_copies_symbols_and_rgba_colors() {
    let cells = [
        cell('A', Rgba::rgb(1, 2, 3), Rgba::rgb(4, 5, 6)),
        cell('B', Rgba::rgb(7, 8, 9), Rgba::rgb(10, 11, 12)),
        cell('C', Rgba::rgb(13, 14, 15), Rgba::rgb(16, 17, 18)),
        cell('D', Rgba::rgb(19, 20, 21), Rgba::rgb(22, 23, 24)),
    ];
    let frame = frame_with(&cells, 2, 2);
    let buffer = frame_to_buffer(&frame, Rect::new(0, 0, 4, 3), ColorMode::TrueColor);

    for (index, expected) in cells.iter().enumerate() {
        let x = (index as u16) % 2;
        let y = (index as u16) / 2;
        let actual = buffer_cell(&buffer, x, y);
        assert_eq!(actual.symbol(), expected.ch.to_string());
        assert_eq!(
            actual.fg,
            Color::Rgb(expected.fg.r, expected.fg.g, expected.fg.b)
        );
        assert_eq!(
            actual.bg,
            Color::Rgb(expected.bg.r, expected.bg.g, expected.bg.b)
        );
    }

    // Cells outside the grid keep the buffer's default blank cell.
    assert_eq!(buffer_cell(&buffer, 3, 2).symbol(), " ");
}

#[test]
fn ansi256_maps_black_white_and_red_to_xterm_indices() {
    let cells = [
        cell('K', Rgba::rgb(0, 0, 0), Rgba::rgb(255, 255, 255)),
        cell('W', Rgba::rgb(255, 255, 255), Rgba::rgb(0, 0, 0)),
        cell('R', Rgba::rgb(255, 0, 0), Rgba::rgb(0, 255, 0)),
    ];
    let frame = frame_with(&cells, 3, 1);
    let buffer = frame_to_buffer(&frame, Rect::new(0, 0, 3, 1), ColorMode::Ansi256);

    // The 6x6x6 cube starts at index 16; pure black is (0,0,0) and pure white
    // is (5,5,5), so their indices are 16 and 231. Pure red is 196.
    assert_eq!(buffer_cell(&buffer, 0, 0).fg, Color::Indexed(16));
    assert_eq!(buffer_cell(&buffer, 0, 0).bg, Color::Indexed(231));
    assert_eq!(buffer_cell(&buffer, 1, 0).fg, Color::Indexed(231));
    assert_eq!(buffer_cell(&buffer, 1, 0).bg, Color::Indexed(16));
    assert_eq!(buffer_cell(&buffer, 2, 0).fg, Color::Indexed(196));
    assert_eq!(buffer_cell(&buffer, 2, 0).bg, Color::Indexed(46));
}

#[test]
fn ansi256_maps_mid_gray_to_the_grayscale_ramp() {
    let cells = [cell('G', Rgba::rgb(128, 128, 128), Rgba::rgb(0, 0, 0))];
    let frame = frame_with(&cells, 1, 1);
    let buffer = frame_to_buffer(&frame, Rect::new(0, 0, 1, 1), ColorMode::Ansi256);

    // Ramp entry 12 is exactly 128, so it beats the nearest cube level (135).
    assert_eq!(buffer_cell(&buffer, 0, 0).fg, Color::Indexed(244));
}

#[test]
fn ansi16_maps_pure_primaries_to_standard_colors() {
    let cells = [
        cell('R', Rgba::rgb(255, 0, 0), Rgba::rgb(0, 0, 0)),
        cell('G', Rgba::rgb(0, 255, 0), Rgba::rgb(0, 0, 0)),
        cell('B', Rgba::rgb(0, 0, 255), Rgba::rgb(0, 0, 0)),
    ];
    let frame = frame_with(&cells, 3, 1);
    let buffer = frame_to_buffer(&frame, Rect::new(0, 0, 3, 1), ColorMode::Ansi16);

    assert_eq!(buffer_cell(&buffer, 0, 0).fg, Color::Red);
    assert_eq!(buffer_cell(&buffer, 1, 0).fg, Color::Green);
    assert_eq!(buffer_cell(&buffer, 2, 0).fg, Color::Blue);
    assert_eq!(buffer_cell(&buffer, 0, 0).bg, Color::Black);
}

#[test]
fn no_color_uses_reset_for_foreground_and_background() {
    let cells = [cell('X', Rgba::rgb(255, 0, 0), Rgba::rgb(0, 0, 255))];
    let frame = frame_with(&cells, 1, 1);
    let buffer = frame_to_buffer(&frame, Rect::new(0, 0, 2, 1), ColorMode::NoColor);

    let actual = buffer_cell(&buffer, 0, 0);
    assert_eq!(actual.symbol(), "X");
    assert_eq!(actual.fg, Color::Reset);
    assert_eq!(actual.bg, Color::Reset);
}

#[test]
fn smaller_area_clips_and_copies_only_the_clipped_region() {
    let cells: Vec<SceneCell> = "abcdefghijkl"
        .chars()
        .enumerate()
        .map(|(index, ch)| cell(ch, Rgba::rgb(index as u8, 0, 0), Rgba::rgb(0, 0, 0)))
        .collect();
    let frame = frame_with(&cells, 4, 3);

    let buffer = frame_to_buffer(&frame, Rect::new(0, 0, 2, 2), ColorMode::TrueColor);
    assert_eq!(buffer.area, Rect::new(0, 0, 2, 2));
    assert_eq!(buffer_cell(&buffer, 0, 0).symbol(), "a");
    assert_eq!(buffer_cell(&buffer, 1, 0).symbol(), "b");
    assert_eq!(buffer_cell(&buffer, 0, 1).symbol(), "e");
    assert_eq!(buffer_cell(&buffer, 1, 1).symbol(), "f");
    assert!(buffer.cell((2, 0)).is_none());

    // A non-zero origin keeps global buffer coordinates.
    let offset = frame_to_buffer(&frame, Rect::new(3, 2, 2, 2), ColorMode::TrueColor);
    assert_eq!(buffer_cell(&offset, 3, 2).symbol(), "a");
    assert_eq!(buffer_cell(&offset, 4, 3).symbol(), "f");
    assert!(offset.cell((0, 0)).is_none());

    // Zero-sized areas and empty grids are legal.
    let empty_area = frame_to_buffer(&frame, Rect::new(0, 0, 0, 0), ColorMode::TrueColor);
    assert_eq!(empty_area.area, Rect::new(0, 0, 0, 0));
    let empty_grid = frame_with(&[], 0, 0);
    let buffer = frame_to_buffer(&empty_grid, Rect::new(0, 0, 2, 2), ColorMode::TrueColor);
    assert_eq!(buffer.area, Rect::new(0, 0, 2, 2));
    assert_eq!(buffer_cell(&buffer, 0, 0).symbol(), " ");
}

#[test]
fn color_mode_precedence_and_fallback() {
    // --no-color wins over every environment hint.
    assert_eq!(
        color_mode_for(true, Some("truecolor"), Some("screen-256color")),
        ColorMode::NoColor
    );
    assert_eq!(color_mode_for(true, None, None), ColorMode::NoColor);

    // COLORTERM advertises truecolor and wins over a 256-color TERM.
    assert_eq!(
        color_mode_for(false, Some("truecolor"), Some("screen-256color")),
        ColorMode::TrueColor
    );
    assert_eq!(
        color_mode_for(false, Some("24bit"), Some("xterm")),
        ColorMode::TrueColor
    );
    assert_eq!(
        color_mode_for(false, Some("TrueColor"), None),
        ColorMode::TrueColor
    );

    // TERM advertises the 256-color palette.
    assert_eq!(
        color_mode_for(false, Some("unknown"), Some("screen-256color")),
        ColorMode::Ansi256
    );
    assert_eq!(
        color_mode_for(false, None, Some("xterm-256color")),
        ColorMode::Ansi256
    );

    // The 16-color palette is the safe fallback.
    assert_eq!(
        color_mode_for(false, Some("unknown"), Some("xterm")),
        ColorMode::Ansi16
    );
    assert_eq!(
        color_mode_for(false, Some(""), Some("dumb")),
        ColorMode::Ansi16
    );
    assert_eq!(color_mode_for(false, None, None), ColorMode::Ansi16);
}

#[test]
fn detect_color_mode_honors_the_forced_flag() {
    assert_eq!(detect_color_mode(true), ColorMode::NoColor);
}
