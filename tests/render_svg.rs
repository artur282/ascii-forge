use ascii_forge::render::RenderOptions;
use ascii_forge::render::svg::{render_svg, write_svg};
use ascii_forge::scene::{Cell, Frame, Grid, Rgba};

const GREEN: Rgba = Rgba::rgb(0, 255, 136);
const RED: Rgba = Rgba::rgb(255, 0, 128);
const BACKGROUND: Rgba = Rgba::rgb(10, 12, 16);

fn frame_from_lines(lines: &[&str]) -> Frame {
    let rows = lines.len() as u16;
    let cols = lines
        .iter()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(0) as u16;
    let mut grid = Grid::blank(cols, rows, GREEN, BACKGROUND);
    for (y, line) in lines.iter().enumerate() {
        for (x, ch) in line.chars().enumerate() {
            let cell = Cell {
                ch,
                fg: GREEN,
                bg: BACKGROUND,
            };
            assert!(grid.set(x as u16, y as u16, cell));
        }
    }
    Frame { index: 0, grid }
}

fn single_row(cells: Vec<Cell>) -> Frame {
    let cols = cells.len() as u16;
    Frame {
        index: 0,
        grid: Grid {
            cols,
            rows: 1,
            cells,
        },
    }
}

fn cell(ch: char, fg: Rgba) -> Cell {
    Cell {
        ch,
        fg,
        bg: BACKGROUND,
    }
}

#[test]
fn renders_an_svg_root_with_text_rows() {
    let frame = frame_from_lines(&["AB", "CD"]);

    let svg = render_svg(&frame, &RenderOptions::default()).expect("render svg");

    assert!(
        svg.starts_with("<svg"),
        "output must start with <svg: {svg}"
    );
    assert!(svg.contains("<text"), "missing <text> element: {svg}");
    assert!(
        svg.contains("xml:space=\"preserve\""),
        "missing xml:space: {svg}"
    );
    assert!(svg.contains("width=\""), "missing width: {svg}");
    assert!(svg.contains("height=\""), "missing height: {svg}");
    assert!(svg.contains("viewBox=\""), "missing viewBox: {svg}");
    assert!(
        svg.contains("font-family=\"DejaVu Sans Mono, monospace\""),
        "missing monospace font family: {svg}"
    );
    assert!(svg.contains(" y=\""), "missing row baseline: {svg}");
}

#[test]
fn emits_one_text_element_per_row() {
    let frame = frame_from_lines(&["A", "B", "C"]);

    let svg = render_svg(&frame, &RenderOptions::default()).expect("render svg");

    assert_eq!(svg.matches("<text").count(), 3, "one <text> per row: {svg}");
    assert_eq!(svg.matches("</text>").count(), 3, "closed text rows: {svg}");
}

#[test]
fn preserves_cell_color_in_tspan_runs() {
    let frame = single_row(vec![cell('A', GREEN), cell('B', GREEN), cell('C', RED)]);

    let svg = render_svg(&frame, &RenderOptions::default()).expect("render svg");

    assert!(
        svg.contains("fill=\"#00ff88\""),
        "missing green fill: {svg}"
    );
    assert!(svg.contains("fill=\"#ff0080\""), "missing red fill: {svg}");
}

#[test]
fn groups_consecutive_cells_with_the_same_color_into_one_run() {
    let frame = single_row(vec![cell('A', GREEN), cell('B', GREEN), cell('C', RED)]);

    let svg = render_svg(&frame, &RenderOptions::default()).expect("render svg");

    assert_eq!(svg.matches("<tspan").count(), 2, "two color runs: {svg}");
    assert!(svg.contains(">AB</tspan>"), "green run merged: {svg}");
    assert!(svg.contains(">C</tspan>"), "red run: {svg}");
}

#[test]
fn keeps_spaces_inside_color_runs() {
    let frame = frame_from_lines(&["A B"]);

    let svg = render_svg(&frame, &RenderOptions::default()).expect("render svg");

    assert_eq!(svg.matches("<tspan").count(), 1, "one run: {svg}");
    assert!(svg.contains(">A B</tspan>"), "spaces preserved: {svg}");
}

#[test]
fn skips_fully_transparent_cells() {
    let frame = single_row(vec![
        cell('A', GREEN),
        cell('Z', Rgba::new(0, 255, 136, 0)),
        cell('B', GREEN),
    ]);

    let svg = render_svg(&frame, &RenderOptions::default()).expect("render svg");

    assert_eq!(
        svg.matches("<tspan").count(),
        2,
        "gap splits the runs: {svg}"
    );
    assert!(!svg.contains('Z'), "transparent cell leaked: {svg}");
    assert!(svg.contains(">A</tspan>"), "first run: {svg}");
    assert!(svg.contains(">B</tspan>"), "second run: {svg}");
    assert!(svg.contains(" x=\""), "gap needs an x offset: {svg}");
}

#[test]
fn escapes_xml_special_characters() {
    let chars = ['&', '<', '>', '"', '\''];
    let frame = single_row(chars.into_iter().map(|ch| cell(ch, GREEN)).collect());

    let svg = render_svg(&frame, &RenderOptions::default()).expect("render svg");

    assert!(svg.contains("&amp;"), "missing &amp;: {svg}");
    assert!(svg.contains("&lt;"), "missing &lt;: {svg}");
    assert!(svg.contains("&gt;"), "missing &gt;: {svg}");
    assert!(svg.contains("&quot;"), "missing &quot;: {svg}");
    assert!(svg.contains("&apos;"), "missing &apos;: {svg}");
    assert!(!svg.contains("&<"), "raw special characters leaked: {svg}");
}

#[test]
fn draws_the_background_rect_by_default() {
    let frame = frame_from_lines(&["A"]);

    let svg = render_svg(&frame, &RenderOptions::default()).expect("render svg");

    assert!(svg.contains("<rect"), "missing background rect: {svg}");
}

#[test]
fn uses_the_configured_background_color() {
    let frame = frame_from_lines(&["A"]);
    let options = RenderOptions {
        background: Some(Rgba::rgb(1, 2, 3)),
        ..RenderOptions::default()
    };

    let svg = render_svg(&frame, &options).expect("render svg");

    assert!(svg.contains("fill=\"#010203\""), "wrong background: {svg}");
}

#[test]
fn omits_the_background_rect_when_transparent() {
    let frame = frame_from_lines(&["A"]);
    let options = RenderOptions {
        transparent: true,
        ..RenderOptions::default()
    };

    let svg = render_svg(&frame, &options).expect("render svg");

    assert!(!svg.contains("<rect"), "transparent has a rect: {svg}");
}

#[test]
fn writes_the_svg_document_to_a_path() {
    let frame = frame_from_lines(&["AB"]);
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("frame.svg");

    write_svg(&frame, &RenderOptions::default(), &path).expect("write svg");

    let written = std::fs::read_to_string(&path).expect("read svg");
    assert!(written.starts_with("<svg"), "written output: {written}");
    assert!(written.contains("<text"), "written output: {written}");
}
