use ascii_forge::figlet::{
    FigletError, FigletOptions, font_names, normalize_text, render_figlet, render_text,
};
use ascii_forge::scene::{Align, Grid, Rgba};

const TRANSPARENT: Rgba = Rgba::new(0, 0, 0, 0);

fn options() -> FigletOptions {
    FigletOptions {
        font: "standard".to_string(),
        color: None,
        max_cols: 80,
        max_rows: 24,
        align: Align::Left,
    }
}

fn row_text(grid: &Grid, y: u16) -> String {
    grid.row(y)
        .expect("row inside grid")
        .iter()
        .map(|cell| cell.ch)
        .collect()
}

fn trimmed_rows(grid: &Grid) -> Vec<String> {
    (0..grid.rows)
        .map(|y| row_text(grid, y).trim_end().to_string())
        .collect()
}

fn content_rows(grid: &Grid) -> Vec<String> {
    (0..grid.rows)
        .map(|y| row_text(grid, y).trim().to_string())
        .collect()
}

fn first_non_space_col(grid: &Grid) -> Option<u16> {
    (0..grid.cols)
        .find(|&x| (0..grid.rows).any(|y| grid.get(x, y).is_some_and(|cell| cell.ch != ' ')))
}

#[test]
fn normalize_text_strips_accents() {
    assert_eq!(
        normalize_text("Héllo, wörld! Ça va?"),
        "Hello, world! Ca va?"
    );
    assert_eq!(normalize_text("ÀÉÎÕÜ àéîõü"), "AEIOU aeiou");
    assert_eq!(normalize_text("café\tcrème"), "cafe creme");
}

#[test]
fn normalize_text_maps_typographic_characters() {
    assert_eq!(
        normalize_text("‘one’ “two” – three — four"),
        "'one' \"two\" - three - four"
    );
    assert_eq!(normalize_text("Waiting…"), "Waiting...");
}

#[test]
fn normalize_text_replaces_unsupported_glyphs() {
    assert_eq!(normalize_text("日本語"), "???");
    assert_eq!(normalize_text("a😀b"), "a?b");
}

#[test]
fn normalize_text_of_empty_and_whitespace_is_empty() {
    assert_eq!(normalize_text(""), "");
    assert_eq!(normalize_text("   "), "");
    assert_eq!(normalize_text(" \t\n\r "), "");
    assert_eq!(normalize_text("\u{0301}"), "");
}

#[test]
fn font_names_lists_built_in_catalog() {
    assert_eq!(font_names(), ["standard", "small", "big", "slant"]);
}

#[test]
fn default_options_are_stable() {
    let defaults = FigletOptions::default();

    assert_eq!(defaults.font, "standard");
    assert_eq!(defaults.color, None);
    assert_eq!(defaults.max_cols, 80);
    assert_eq!(defaults.max_rows, 24);
    assert_eq!(defaults.align, Align::Center);
}

#[test]
fn figlet_af_renders_non_empty_grid_within_bounds() {
    let grid = render_figlet("AF", &options()).expect("standard font renders AF");

    assert_eq!(grid.cols, 80);
    assert!((1..=24).contains(&grid.rows));
    assert!(grid.cells.iter().any(|cell| cell.ch != ' '));
    assert!(grid.cells.iter().all(|cell| cell.bg == TRANSPARENT));
    assert!(
        grid.cells
            .iter()
            .filter(|cell| cell.ch != ' ')
            .all(|cell| cell.fg == Rgba::default())
    );
}

#[test]
fn figlet_uses_requested_color_and_transparent_background() {
    let red = Rgba::rgb(255, 0, 0);
    let options = FigletOptions {
        color: Some(red),
        ..options()
    };
    let grid = render_figlet("AF", &options).expect("standard font renders AF");

    assert!(grid.cells.iter().all(|cell| cell.bg == TRANSPARENT));
    let glyphs: Vec<_> = grid.cells.iter().filter(|cell| cell.ch != ' ').collect();
    assert!(!glyphs.is_empty());
    assert!(glyphs.iter().all(|cell| cell.fg == red));
}

#[test]
fn figlet_align_moves_the_block() {
    let left = render_figlet("AF", &options()).expect("left align");
    let center = render_figlet(
        "AF",
        &FigletOptions {
            align: Align::Center,
            ..options()
        },
    )
    .expect("center align");
    let right = render_figlet(
        "AF",
        &FigletOptions {
            align: Align::Right,
            ..options()
        },
    )
    .expect("right align");

    assert_eq!(content_rows(&left), content_rows(&center));
    assert_eq!(content_rows(&left), content_rows(&right));

    let left_x = first_non_space_col(&left).expect("left has glyphs");
    let center_x = first_non_space_col(&center).expect("center has glyphs");
    let right_x = first_non_space_col(&right).expect("right has glyphs");

    assert!(left_x < center_x);
    assert!(center_x < right_x);
    assert_eq!(center_x - left_x, (right_x - left_x) / 2);
}

#[test]
fn figlet_falls_back_to_small_font_when_requested_is_too_tall() {
    let options = FigletOptions {
        font: "big".to_string(),
        max_cols: 40,
        max_rows: 5,
        ..options()
    };
    let grid = render_figlet("AF", &options).expect("small font fits the canvas");

    assert_eq!(grid.cols, 40);
    assert_eq!(grid.rows, 4);
}

#[test]
fn figlet_narrow_canvas_falls_back_or_scales() {
    let options = FigletOptions {
        max_cols: 4,
        max_rows: 4,
        ..options()
    };

    match render_figlet("AF", &options) {
        Ok(grid) => {
            assert_eq!(grid.cols, 4);
            assert!((1..=4).contains(&grid.rows));
            assert!(grid.cells.iter().any(|cell| cell.ch != ' '));
        }
        Err(FigletError::DoesNotFit { cols, rows }) => assert_eq!((cols, rows), (4, 4)),
        Err(other) => panic!("unexpected error: {other}"),
    }
}

#[test]
fn figlet_wide_text_scales_into_bounds() {
    let options = FigletOptions {
        max_cols: 20,
        max_rows: 3,
        align: Align::Right,
        ..options()
    };
    let grid = render_figlet("WIDE TEXT", &options).expect("wide text scales down");

    assert_eq!(grid.cols, 20);
    assert!((1..=3).contains(&grid.rows));
    assert!(grid.cells.iter().any(|cell| cell.ch != ' '));
}

#[test]
fn figlet_zero_canvas_is_does_not_fit() {
    let options = FigletOptions {
        max_cols: 0,
        max_rows: 10,
        ..options()
    };

    assert!(matches!(
        render_figlet("AF", &options),
        Err(FigletError::DoesNotFit { cols: 0, rows: 10 })
    ));
}

#[test]
fn figlet_unknown_font_is_reported() {
    let options = FigletOptions {
        font: "not-a-font".to_string(),
        ..options()
    };

    assert!(matches!(
        render_figlet("AF", &options),
        Err(FigletError::UnknownFont(name)) if name == "not-a-font"
    ));
}

fn minimal_font() -> String {
    let mut font = String::from("flf2a$ 2 1 4 0 0\n");
    for _ in 32..=126u32 {
        font.push_str("AA@\n");
        font.push_str("AA@@\n");
    }
    font
}

#[test]
fn figlet_loads_custom_flf_font_from_path() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let path = dir.path().join("custom.flf");
    std::fs::write(&path, minimal_font()).expect("write custom font");
    let options = FigletOptions {
        font: path.to_string_lossy().into_owned(),
        max_cols: 12,
        max_rows: 5,
        ..options()
    };

    let grid = render_figlet("AF", &options).expect("custom font renders");
    assert_eq!(grid.cols, 12);
    assert_eq!(grid.rows, 2);
    assert!(grid.cells.iter().any(|cell| cell.ch == 'A'));
}

#[test]
fn figlet_font_ending_in_flf_is_treated_as_a_path() {
    let options = FigletOptions {
        font: "missing-font.flf".to_string(),
        ..options()
    };

    assert!(matches!(
        render_figlet("AF", &options),
        Err(FigletError::UnknownFont(name)) if name == "missing-font.flf"
    ));
}

#[test]
fn render_text_wraps_on_max_cols() {
    let options = FigletOptions {
        max_cols: 10,
        max_rows: 5,
        ..options()
    };
    let grid = render_text("the quick brown fox", &options).expect("text wraps");

    assert_eq!(grid.cols, 10);
    assert_eq!(trimmed_rows(&grid), ["the quick", "brown fox"]);
}

#[test]
fn render_text_hard_breaks_overlong_words() {
    let options = FigletOptions {
        max_cols: 4,
        max_rows: 3,
        ..options()
    };
    let grid = render_text("abcdefghij", &options).expect("long word hard breaks");

    assert_eq!(grid.cols, 4);
    assert_eq!(trimmed_rows(&grid), ["abcd", "efgh", "ij"]);
}

#[test]
fn render_text_aligns_left_center_right() {
    let options = FigletOptions {
        max_cols: 10,
        max_rows: 1,
        ..options()
    };

    let left = render_text("hi", &options).expect("left align");
    let center = render_text(
        "hi",
        &FigletOptions {
            align: Align::Center,
            ..options.clone()
        },
    )
    .expect("center align");
    let right = render_text(
        "hi",
        &FigletOptions {
            align: Align::Right,
            ..options.clone()
        },
    )
    .expect("right align");

    assert_eq!(row_text(&left, 0), "hi        ");
    assert_eq!(row_text(&center, 0), "    hi    ");
    assert_eq!(row_text(&right, 0), "        hi");
}

#[test]
fn render_text_does_not_fit_when_rows_are_exhausted() {
    let options = FigletOptions {
        max_cols: 5,
        max_rows: 1,
        ..options()
    };

    assert!(matches!(
        render_text("hello world", &options),
        Err(FigletError::DoesNotFit { cols: 5, rows: 1 })
    ));
}

#[test]
fn empty_text_renders_empty_grid() {
    let options = options();

    let text = render_text("", &options).expect("empty text");
    assert_eq!((text.cols, text.rows), (0, 0));
    assert!(text.cells.is_empty());

    let figlet = render_figlet("", &options).expect("empty figlet text");
    assert_eq!((figlet.cols, figlet.rows), (0, 0));
    assert!(figlet.cells.is_empty());
}
