//! Integration tests for the streaming GIF exporter (issue #9, spec S4).
//!
//! The exporter is exercised through its public API only. Assertions are
//! structural (frame count, dimensions, delays, repeat mode, transparency and
//! byte-level determinism) so palette choices remain free to improve without
//! breaking the tests.

use std::fs::{self, File};
use std::io::BufReader;
use std::path::Path;

use ascii_forge::encode::gif::{EncodeError, GifEncoder, GifOptions, write_gif};
use image::RgbaImage;

const WIDTH: u16 = 8;
const HEIGHT: u16 = 8;

/// Deterministic gradient frame tinted by `step`.
fn sample_frame(step: u8) -> RgbaImage {
    let mut image = RgbaImage::new(u32::from(WIDTH), u32::from(HEIGHT));
    for (x, y, pixel) in image.enumerate_pixels_mut() {
        *pixel = image::Rgba([
            step.wrapping_mul(17).wrapping_add((x as u8) * 8),
            (y as u8) * 16,
            step.wrapping_mul(31),
            255,
        ]);
    }
    image
}

/// Frame where the left half is opaque and the right half is transparent.
fn half_transparent_frame() -> RgbaImage {
    let mut image = RgbaImage::new(u32::from(WIDTH), u32::from(HEIGHT));
    for (x, _, pixel) in image.enumerate_pixels_mut() {
        *pixel = if x < u32::from(WIDTH) / 2 {
            image::Rgba([200, 30, 40, 255])
        } else {
            image::Rgba([0, 0, 0, 0])
        };
    }
    image
}

struct DecodedFrame {
    width: u16,
    height: u16,
    delay: u16,
    transparent: Option<u8>,
    palette_len: usize,
    indices: Vec<u8>,
}

struct DecodedGif {
    width: u16,
    height: u16,
    repeat: gif::Repeat,
    frames: Vec<DecodedFrame>,
}

/// Decode every frame of `path` into owned data so assertions never hold a
/// borrow into the decoder.
fn decode(path: &Path) -> DecodedGif {
    let file = File::open(path).expect("open gif");
    let mut decoder = gif::Decoder::new(BufReader::new(file)).expect("read gif header");
    let width = decoder.width();
    let height = decoder.height();
    let repeat = decoder.repeat();

    let mut frames = Vec::new();
    while let Some(frame) = decoder.read_next_frame().expect("decode gif frame") {
        frames.push(DecodedFrame {
            width: frame.width,
            height: frame.height,
            delay: frame.delay,
            transparent: frame.transparent,
            palette_len: frame.palette.as_ref().map_or(0, |table| table.len() / 3),
            indices: frame.buffer.to_vec(),
        });
    }

    DecodedGif {
        width,
        height,
        repeat,
        frames,
    }
}

#[test]
fn default_options_match_the_documented_contract() {
    let options = GifOptions::default();

    assert_eq!(options.fps, 30);
    assert!(options.loop_);
    assert_eq!(options.palette_size, 256);
    assert!(!options.transparent);
}

#[test]
fn three_frames_decode_with_expected_shape_and_delay() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("three.gif");
    let frames = vec![sample_frame(0), sample_frame(1), sample_frame(2)];

    write_gif(&path, WIDTH, HEIGHT, &GifOptions::default(), frames).expect("write gif");

    let gif = decode(&path);
    assert_eq!((gif.width, gif.height), (WIDTH, HEIGHT));
    assert_eq!(gif.frames.len(), 3);
    for (index, frame) in gif.frames.iter().enumerate() {
        assert_eq!(
            (frame.width, frame.height),
            (WIDTH, HEIGHT),
            "frame {index}"
        );
        // 30 fps => 100 / 30 = 3.33 hundredths of a second => 3.
        assert_eq!(frame.delay, 3, "frame {index} delay");
    }
}

#[test]
fn high_fps_never_encodes_a_zero_delay() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("fast.gif");
    let options = GifOptions {
        fps: 250,
        ..GifOptions::default()
    };

    write_gif(
        &path,
        WIDTH,
        HEIGHT,
        &options,
        std::iter::once(sample_frame(0)),
    )
    .expect("write gif");

    // 100 / 250 = 0.4 rounds to 0; the delay must be clamped to 1.
    assert_eq!(decode(&path).frames[0].delay, 1);
}

#[test]
fn identical_frames_produce_identical_bytes() {
    let dir = tempfile::tempdir().expect("temp dir");
    let first = dir.path().join("first.gif");
    let second = dir.path().join("second.gif");
    let options = GifOptions::default();

    write_gif(&first, WIDTH, HEIGHT, &options, (0..3).map(sample_frame)).expect("write first gif");
    write_gif(&second, WIDTH, HEIGHT, &options, (0..3).map(sample_frame))
        .expect("write second gif");

    let first_bytes = fs::read(&first).expect("read first gif");
    let second_bytes = fs::read(&second).expect("read second gif");
    assert!(!first_bytes.is_empty());
    assert_eq!(first_bytes, second_bytes);
}

#[test]
fn transparent_option_reserves_palette_index_zero() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("transparent.gif");
    let options = GifOptions {
        palette_size: 4,
        transparent: true,
        ..GifOptions::default()
    };

    write_gif(
        &path,
        WIDTH,
        HEIGHT,
        &options,
        std::iter::once(half_transparent_frame()),
    )
    .expect("write gif");

    let gif = decode(&path);
    let frame = &gif.frames[0];
    assert_eq!(frame.transparent, Some(0));
    assert!(
        frame.palette_len <= 4,
        "palette table {} exceeds the configured size",
        frame.palette_len
    );
    assert!(
        frame.indices[0] > 0,
        "opaque pixels must not use the transparent index"
    );
    assert_eq!(
        frame.indices[usize::from(WIDTH) - 1],
        0,
        "transparent pixels map to palette index 0"
    );
}

#[test]
fn alpha_threshold_is_one_hundred_and_twenty_eight() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("threshold.gif");
    let mut image = RgbaImage::new(u32::from(WIDTH), u32::from(HEIGHT));
    for (_, _, pixel) in image.enumerate_pixels_mut() {
        *pixel = image::Rgba([10, 20, 30, 255]);
    }
    image.put_pixel(0, 0, image::Rgba([70, 80, 90, 127]));
    image.put_pixel(1, 0, image::Rgba([200, 100, 50, 128]));

    write_gif(
        &path,
        WIDTH,
        HEIGHT,
        &GifOptions {
            transparent: true,
            ..GifOptions::default()
        },
        std::iter::once(image),
    )
    .expect("write gif");

    let gif = decode(&path);
    let frame = &gif.frames[0];
    assert_eq!(frame.transparent, Some(0));
    assert_eq!(frame.indices[0], 0, "alpha 127 is transparent");
    assert!(
        frame.indices[1] > 0,
        "alpha 128 is opaque and must not use the transparent index"
    );
}

#[test]
fn palette_size_two_keeps_one_opaque_color_plus_transparency() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("two-colors.gif");
    let options = GifOptions {
        palette_size: 2,
        transparent: true,
        ..GifOptions::default()
    };

    write_gif(
        &path,
        WIDTH,
        HEIGHT,
        &options,
        std::iter::once(half_transparent_frame()),
    )
    .expect("write gif");

    let gif = decode(&path);
    let frame = &gif.frames[0];
    assert_eq!(frame.transparent, Some(0));
    assert_eq!(
        frame.palette_len, 2,
        "one transparent plus one opaque color"
    );
    let half = usize::from(WIDTH) / 2;
    for (x, &index) in frame.indices.iter().enumerate() {
        let expected = if x % usize::from(WIDTH) < half { 1 } else { 0 };
        assert_eq!(index, expected, "pixel {x}");
    }
}

#[test]
fn fully_transparent_frames_stay_valid() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("empty.gif");

    write_gif(
        &path,
        WIDTH,
        HEIGHT,
        &GifOptions {
            transparent: true,
            ..GifOptions::default()
        },
        // `RgbaImage::new` starts fully transparent.
        std::iter::once(RgbaImage::new(u32::from(WIDTH), u32::from(HEIGHT))),
    )
    .expect("write gif");

    let gif = decode(&path);
    let frame = &gif.frames[0];
    assert_eq!(frame.transparent, Some(0));
    assert!(frame.palette_len >= 1);
    assert!(frame.indices.iter().all(|&index| index == 0));
}

#[test]
fn opaque_frames_have_no_transparent_index() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("opaque.gif");
    let options = GifOptions {
        transparent: true,
        ..GifOptions::default()
    };

    write_gif(
        &path,
        WIDTH,
        HEIGHT,
        &options,
        std::iter::once(sample_frame(2)),
    )
    .expect("write gif");

    assert_eq!(decode(&path).frames[0].transparent, None);
}

#[test]
fn write_gif_consumes_a_lazy_iterator_and_writes_every_frame() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("lazy.gif");
    let produced = std::rc::Rc::new(std::cell::Cell::new(0_u8));
    let counter = std::rc::Rc::clone(&produced);
    let frames = std::iter::from_fn(move || {
        let step = counter.get();
        if step == 5 {
            return None;
        }
        counter.set(step + 1);
        Some(sample_frame(step))
    });

    write_gif(&path, WIDTH, HEIGHT, &GifOptions::default(), frames).expect("write gif");

    assert_eq!(
        produced.get(),
        5,
        "the iterator must be driven exactly once"
    );
    assert_eq!(decode(&path).frames.len(), 5);
}

#[test]
fn invalid_options_are_rejected() {
    let cases = [
        GifOptions {
            fps: 0,
            ..GifOptions::default()
        },
        GifOptions {
            palette_size: 1,
            ..GifOptions::default()
        },
        GifOptions {
            palette_size: 300,
            ..GifOptions::default()
        },
    ];

    for (index, options) in cases.into_iter().enumerate() {
        let Err(error) = GifEncoder::new(Vec::new(), WIDTH, HEIGHT, options) else {
            panic!("case {index} must be rejected");
        };
        assert!(
            matches!(error, EncodeError::InvalidOptions { .. }),
            "case {index} produced {error}"
        );
    }

    for (width, height) in [(0, HEIGHT), (WIDTH, 0)] {
        let Err(error) = GifEncoder::new(Vec::new(), width, height, GifOptions::default()) else {
            panic!("{width}x{height} must be rejected");
        };
        assert!(
            matches!(error, EncodeError::InvalidOptions { .. }),
            "{width}x{height} produced {error}"
        );
    }

    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("invalid.gif");
    let Err(error) = write_gif(
        &path,
        WIDTH,
        HEIGHT,
        &GifOptions {
            fps: 0,
            ..GifOptions::default()
        },
        std::iter::empty(),
    ) else {
        panic!("write_gif must reject invalid options");
    };
    assert!(matches!(error, EncodeError::InvalidOptions { .. }));
    assert!(
        !path.exists(),
        "invalid options must not create the output file"
    );
}

#[test]
fn mismatched_frame_dimensions_are_rejected() {
    let mut sink = Vec::new();
    let mut encoder =
        GifEncoder::new(&mut sink, WIDTH, HEIGHT, GifOptions::default()).expect("encoder");

    let error = encoder
        .write_frame(&RgbaImage::new(4, 4))
        .expect_err("mismatched frame must be rejected");

    assert!(
        matches!(error, EncodeError::Dimensions { .. }),
        "unexpected error: {error}"
    );
}

#[test]
fn loop_option_maps_to_gif_repeat() {
    let dir = tempfile::tempdir().expect("temp dir");

    let looping = dir.path().join("loop.gif");
    write_gif(
        &looping,
        WIDTH,
        HEIGHT,
        &GifOptions::default(),
        std::iter::once(sample_frame(0)),
    )
    .expect("write looping gif");
    assert_eq!(decode(&looping).repeat, gif::Repeat::Infinite);

    let once = dir.path().join("once.gif");
    let options = GifOptions {
        loop_: false,
        ..GifOptions::default()
    };
    write_gif(
        &once,
        WIDTH,
        HEIGHT,
        &options,
        std::iter::once(sample_frame(1)),
    )
    .expect("write single-play gif");
    // `Finite(0)` is the gif crate's spelling of "no repeat extension", which
    // players interpret as a single play.
    assert_eq!(decode(&once).repeat, gif::Repeat::Finite(0));
}

#[test]
fn palette_size_limits_the_color_table() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("limited.gif");
    let mut image = RgbaImage::new(u32::from(WIDTH), u32::from(HEIGHT));
    for (x, y, pixel) in image.enumerate_pixels_mut() {
        *pixel = image::Rgba([(x as u8) * 32, (y as u8) * 32, ((x + y) as u8) * 16, 255]);
    }
    let options = GifOptions {
        palette_size: 4,
        ..GifOptions::default()
    };

    write_gif(&path, WIDTH, HEIGHT, &options, std::iter::once(image)).expect("write gif");

    let palette_len = decode(&path).frames[0].palette_len;
    assert!(
        (1..=4).contains(&palette_len),
        "expected a small color table, got {palette_len}"
    );
}
