//! Image import tests for Phase 5.

use std::path::Path;

use ascii_forge::import::{
    ASCII_CHARSET, ImportOptions, build_static_scene, image_to_grid, import_image, resolve_charset,
    rows_for,
};
use ascii_forge::scene::{LayerKind, OutputFormat};
use image::{DynamicImage, Rgba, RgbaImage};

fn gradient(width: u32, height: u32) -> DynamicImage {
    let mut image = RgbaImage::new(width, height);
    for y in 0..height {
        for x in 0..width {
            let value = ((x * 255) / width.max(1)) as u8;
            image.put_pixel(x, y, Rgba([value, value, value, 255]));
        }
    }
    DynamicImage::ImageRgba8(image)
}

fn color_image(width: u32, height: u32) -> DynamicImage {
    let mut image = RgbaImage::new(width, height);
    for y in 0..height {
        for x in 0..width {
            let red = if x < width / 2 { 255 } else { 0 };
            let green = if x < width / 2 { 0 } else { 255 };
            image.put_pixel(x, y, Rgba([red, green, 0, 255]));
        }
    }
    DynamicImage::ImageRgba8(image)
}

fn options() -> ImportOptions {
    ImportOptions {
        cols: 40,
        ..ImportOptions::default()
    }
}

#[test]
fn rows_preserve_source_and_cell_aspect() {
    assert_eq!(rows_for(40, 64, 32, 0.542), 11);
    assert_eq!(rows_for(80, 1920, 1080, 0.542), 24);
    assert_eq!(rows_for(40, 0, 32, 0.542), 1);
}

#[test]
fn image_converts_to_expected_dimensions() {
    let grid = image_to_grid(&gradient(64, 32), &options()).expect("image grid");

    assert_eq!(grid.cols, 40);
    assert_eq!(grid.rows, 11);
    assert_eq!(grid.cells.len(), 440);
}

#[test]
fn explicit_rows_override_aspect_derivation() {
    let options = ImportOptions {
        rows: Some(7),
        ..options()
    };
    let grid = image_to_grid(&gradient(64, 32), &options).expect("image grid");

    assert_eq!(grid.cols, 40);
    assert_eq!(grid.rows, 7);
}

#[test]
fn luminance_ramp_uses_the_charset() {
    let black = DynamicImage::ImageRgba8(RgbaImage::from_pixel(8, 8, Rgba([0, 0, 0, 255])));
    let white = DynamicImage::ImageRgba8(RgbaImage::from_pixel(8, 8, Rgba([255, 255, 255, 255])));
    let options = ImportOptions {
        cols: 4,
        rows: Some(2),
        charset: ASCII_CHARSET.to_string(),
        ..ImportOptions::default()
    };

    let black_grid = image_to_grid(&black, &options).expect("black");
    let white_grid = image_to_grid(&white, &options).expect("white");
    assert!(black_grid.cells.iter().all(|cell| cell.ch == ' '));
    assert!(white_grid.cells.iter().all(|cell| cell.ch == '@'));
}

#[test]
fn color_and_monochrome_produce_different_valid_cells() {
    let image = color_image(64, 32);
    let mono = image_to_grid(&image, &options()).expect("mono");
    let color_options = ImportOptions {
        color: true,
        ..options()
    };
    let color = image_to_grid(&image, &color_options).expect("color");

    assert!(
        mono.cells
            .iter()
            .all(|cell| cell.fg.r == 255 && cell.fg.g == 255)
    );
    assert!(
        color
            .cells
            .iter()
            .any(|cell| cell.fg.r > cell.fg.g || cell.fg.g > cell.fg.r)
    );
    assert_eq!(mono.cols, color.cols);
    assert_eq!(mono.rows, color.rows);
}

#[test]
fn braille_uses_the_unicode_braille_range() {
    let options = ImportOptions {
        braille: true,
        ..options()
    };
    let grid = image_to_grid(&gradient(64, 32), &options).expect("braille");

    assert!(grid.cells.iter().all(|cell| {
        let value = cell.ch as u32;
        (0x2800..=0x28ff).contains(&value)
    }));
    assert!(grid.cells.iter().any(|cell| cell.ch != '\u{2800}'));
}

#[test]
fn dithering_changes_a_gradient_ramp() {
    let image = gradient(64, 32);
    let plain = image_to_grid(&image, &options()).expect("plain");
    let dithered = image_to_grid(
        &image,
        &ImportOptions {
            dither: true,
            ..options()
        },
    )
    .expect("dithered");

    assert_ne!(
        plain.cells.iter().map(|cell| cell.ch).collect::<Vec<_>>(),
        dithered
            .cells
            .iter()
            .map(|cell| cell.ch)
            .collect::<Vec<_>>()
    );
}

#[test]
fn edges_produce_directional_glyphs() {
    let image = color_image(64, 32);
    let grid = image_to_grid(
        &image,
        &ImportOptions {
            edges: true,
            ..options()
        },
    )
    .expect("edges");

    assert!(
        grid.cells
            .iter()
            .any(|cell| matches!(cell.ch, '|' | '/' | '-' | '\\'))
    );
}

#[test]
fn charset_presets_and_custom_ramps_work() {
    assert_eq!(resolve_charset(None).unwrap(), ASCII_CHARSET);
    assert!(resolve_charset(Some("blocks")).unwrap().contains('█'));
    assert!(resolve_charset(Some("detailed")).unwrap().contains('@'));
    assert_eq!(resolve_charset(Some("@.")).unwrap(), "@.");
    assert_eq!(resolve_charset(Some(" .#")).unwrap(), " .#");
    assert!(resolve_charset(Some("")).is_err());
}

#[test]
fn invalid_combinations_are_rejected() {
    assert!(
        ImportOptions {
            braille: true,
            dither: true,
            ..ImportOptions::default()
        }
        .validate()
        .is_err()
    );
    assert!(
        ImportOptions {
            braille: true,
            edges: true,
            ..ImportOptions::default()
        }
        .validate()
        .is_err()
    );
    assert!(
        ImportOptions {
            charset: " ".to_string(),
            ..ImportOptions::default()
        }
        .validate()
        .is_err()
    );
}

#[test]
fn static_scene_is_valid_and_renderable() {
    let grid = image_to_grid(&gradient(64, 32), &options()).expect("grid");
    let scene = build_static_scene(grid, &options()).expect("scene");

    assert_eq!(scene.timeline.frames, 1);
    assert_eq!(scene.output.format, OutputFormat::Png);
    assert!(scene.output.width >= 16);
    assert!(scene.validate().is_ok());
    assert!(matches!(scene.layers[0].kind, LayerKind::Grid(_)));
}

#[test]
fn image_file_import_works() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("gradient.png");
    gradient(64, 32).save(&path).expect("save fixture");
    let grid = import_image(Path::new(&path), &options()).expect("import");

    assert_eq!(grid.cols, 40);
    assert_eq!(grid.rows, 11);
}
