//! Integration tests for PNG/JPEG rasterization and ANSI output (issue #3, spec S3).

use ab_glyph::{Font, FontVec, PxScale, ScaleFont};
use ascii_forge::render::{RenderError, RenderOptions, raster};
use ascii_forge::scene::{Cell, Frame, Grid, Rgba};

const COLS: u16 = 4;
const ROWS: u16 = 3;
const BG: Rgba = Rgba::rgb(10, 12, 16);
const FG: Rgba = Rgba::rgb(0, 255, 136);
const TEXT: &str = "Ai#";

const FONT_CANDIDATES: [&str; 3] = [
    "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
    "/usr/share/fonts/TTF/DejaVuSansMono.ttf",
    "/System/Library/Fonts/Menlo.ttc",
];

fn sample_frame() -> Frame {
    let mut grid = Grid::blank(COLS, ROWS, FG, BG);
    for (x, ch) in TEXT.chars().enumerate() {
        let cell = Cell {
            ch,
            fg: Rgba::rgb(255, 64, 0),
            bg: BG,
        };
        grid.set(x as u16, 1, cell);
    }
    Frame { index: 0, grid }
}

fn system_font() -> FontVec {
    let bytes = FONT_CANDIDATES
        .iter()
        .find_map(|path| std::fs::read(path).ok())
        .expect("a monospace font must be installed for raster tests");
    FontVec::try_from_vec(bytes).expect("load system font")
}

fn expected_cell_size(font_size: f32, scale: f32) -> (u32, u32) {
    let font = system_font();
    let scaled = font.as_scaled(PxScale::from(font_size * scale));
    let width = scaled.h_advance(scaled.glyph_id('M')).ceil() as u32;
    let height = (scaled.ascent() - scaled.descent() + scaled.line_gap()).ceil() as u32;
    (width, height)
}

#[test]
fn png_dimensions_follow_cell_metrics() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("frame.png");
    let options = RenderOptions::default();

    raster::render_png(&sample_frame(), &options, &path).expect("render png");

    let image = image::open(&path).expect("open png").to_rgba8();
    let (cell_w, cell_h) = expected_cell_size(options.font_size, options.scale);
    assert_eq!(
        image.dimensions(),
        (u32::from(COLS) * cell_w, u32::from(ROWS) * cell_h)
    );
}

#[test]
fn jpeg_is_written_to_disk() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("frame.jpg");

    raster::render_jpeg(&sample_frame(), &RenderOptions::default(), &path).expect("render jpeg");

    let metadata = std::fs::metadata(&path).expect("jpeg exists");
    assert!(metadata.len() > 0, "jpeg must not be empty");
}

#[test]
fn ansi_emits_truecolor_and_text() {
    let frame = sample_frame();

    let ansi = raster::frame_to_ansi(&frame, true);

    assert!(ansi.contains("\x1b[38;2;"), "missing truecolor fg");
    assert!(ansi.contains("\x1b[48;2;"), "missing truecolor bg");
    assert!(ansi.contains(TEXT), "missing text");
    assert!(
        ansi.contains("\x1b[38;2;255;64;0m"),
        "cell fg color not emitted"
    );
    assert_eq!(
        ansi.matches('\n').count(),
        usize::from(ROWS),
        "one line per row"
    );
    assert!(
        ansi.lines().all(|line| line.ends_with("\x1b[0m")),
        "every line must reset SGR"
    );
}

#[test]
fn plain_text_matches_grid_text() {
    let frame = sample_frame();

    let plain = raster::frame_to_ansi(&frame, false);

    assert_eq!(plain, frame.grid.to_text());
    assert_eq!(raster::frame_to_text(&frame), frame.grid.to_text());
    assert!(!plain.contains('\x1b'), "plain text must not contain ANSI");
}

#[test]
fn transparent_output_leaves_blank_cells_clear() {
    let frame = sample_frame();
    let options = RenderOptions {
        transparent: true,
        ..RenderOptions::default()
    };

    let image = raster::frame_to_rgba(&frame, &options).expect("render rgba");
    let (cell_w, cell_h) = expected_cell_size(options.font_size, options.scale);
    let blank_x = u32::from(COLS - 1) * cell_w + cell_w / 2;
    let blank_y = cell_h / 2;

    assert_eq!(
        image.get_pixel(blank_x, blank_y).0[3],
        0,
        "blank cell alpha"
    );

    let opaque = raster::frame_to_rgba(&frame, &RenderOptions::default()).expect("render rgba");
    assert_eq!(
        opaque.get_pixel(blank_x, blank_y).0,
        BG.to_array(),
        "opaque cells use the canvas background"
    );
}

#[test]
fn glyphs_use_foreground_override() {
    let frame = sample_frame();
    let foreground = Rgba::rgb(0, 128, 255);
    let options = RenderOptions {
        foreground: Some(foreground),
        transparent: true,
        ..RenderOptions::default()
    };

    let image = raster::frame_to_rgba(&frame, &options).expect("render rgba");
    let (cell_w, cell_h) = expected_cell_size(options.font_size, options.scale);
    let mut painted = Vec::new();
    for y in cell_h..cell_h * 2 {
        for x in 0..cell_w {
            let pixel = image.get_pixel(x, y).0;
            if pixel[3] > 0 {
                painted.push(pixel);
            }
        }
    }

    assert!(!painted.is_empty(), "the glyph must be drawn");
    for pixel in painted {
        assert!(pixel[0] < 10, "red channel follows foreground");
        assert!((i32::from(pixel[1]) - 128).abs() <= 2, "green channel");
        assert!(pixel[2] > 245, "blue channel");
    }
}

#[test]
fn missing_font_reports_font_error() {
    let options = RenderOptions {
        font_path: Some("/nonexistent/ascii-forge-missing.ttf".to_string()),
        ..RenderOptions::default()
    };

    let error =
        raster::frame_to_rgba(&sample_frame(), &options).expect_err("missing font must fail");

    assert!(
        matches!(error, RenderError::Font(_)),
        "unexpected error: {error}"
    );
    assert!(!error.to_string().is_empty());
}
