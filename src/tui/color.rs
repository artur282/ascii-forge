//! Color capability detection and palette mapping.

use crate::scene::Rgba;
use ratatui::style::Color;

/// How the preview renders cell colors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorMode {
    /// Full 24-bit color.
    TrueColor,
    /// The xterm 256-color palette.
    Ansi256,
    /// The 16 standard ANSI colors.
    Ansi16,
    /// No color; every cell uses `Color::Reset`.
    NoColor,
}

/// Detect the terminal color mode from `COLORTERM`, `TERM` and `--no-color`.
pub fn detect_color_mode(no_color: bool) -> ColorMode {
    color_mode_for(
        no_color,
        std::env::var("COLORTERM").ok().as_deref(),
        std::env::var("TERM").ok().as_deref(),
    )
}

/// Pure precedence rule behind [`detect_color_mode`].
///
/// `--no-color` wins over every hint. `COLORTERM` advertising truecolor wins
/// over `TERM`; a `TERM` without 256-color support falls back to ANSI 16.
pub fn color_mode_for(no_color: bool, colorterm: Option<&str>, term: Option<&str>) -> ColorMode {
    if no_color {
        return ColorMode::NoColor;
    }
    if colorterm.is_some_and(|value| {
        let value = value.to_ascii_lowercase();
        value.contains("truecolor") || value.contains("24bit")
    }) {
        return ColorMode::TrueColor;
    }
    if term.is_some_and(|value| value.to_ascii_lowercase().contains("256color")) {
        return ColorMode::Ansi256;
    }
    ColorMode::Ansi16
}

/// The six intensity levels used by the xterm 256-color cube.
const CUBE_LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];

/// Index of the first grayscale ramp entry in the xterm palette.
const GRAY_INDEX_START: u8 = 232;

/// First grayscale level; xterm adds 10 per ramp entry.
const GRAY_LEVEL_START: i32 = 8;

/// Distance between consecutive grayscale levels.
const GRAY_LEVEL_STEP: i32 = 10;

/// Number of entries in the grayscale ramp.
const GRAY_LEVEL_COUNT: i32 = 24;

/// Nearest xterm 256-color index for `color`.
pub(crate) fn xterm_index(color: Rgba) -> u8 {
    let red = cube_level(color.r);
    let green = cube_level(color.g);
    let blue = cube_level(color.b);
    let cube = [
        CUBE_LEVELS[usize::from(red)],
        CUBE_LEVELS[usize::from(green)],
        CUBE_LEVELS[usize::from(blue)],
    ];
    let cube_index = 16 + 36 * red + 6 * green + blue;

    // The nearest grayscale entry is the one closest to the channel mean.
    let mean = ((u16::from(color.r) + u16::from(color.g) + u16::from(color.b)) / 3) as u8;
    let steps = gray_steps(mean);
    let gray = u8::try_from(GRAY_LEVEL_START + GRAY_LEVEL_STEP * steps).unwrap_or(u8::MAX);

    if channel_distance(color, [gray; 3]) < channel_distance(color, cube) {
        GRAY_INDEX_START + u8::try_from(steps).unwrap_or(0)
    } else {
        cube_index
    }
}

fn cube_level(value: u8) -> u8 {
    let mut best_index = 0;
    let mut best_distance = i32::MAX;
    for (index, level) in CUBE_LEVELS.iter().enumerate() {
        let distance = (i32::from(*level) - i32::from(value)).abs();
        if distance < best_distance {
            best_distance = distance;
            best_index = index as u8;
        }
    }
    best_index
}

fn gray_steps(value: u8) -> i32 {
    let shifted = i32::from(value) - GRAY_LEVEL_START + GRAY_LEVEL_STEP / 2;
    (shifted / GRAY_LEVEL_STEP).clamp(0, GRAY_LEVEL_COUNT - 1)
}

/// RGB approximations of the 16 standard ANSI colors.
///
/// The normal colors use full-intensity primaries so pure red, green and blue
/// map to their normal (not bright) ANSI color; the bright entries are lighter
/// tints of the same hues.
const ANSI16_PALETTE: [(Color, [u8; 3]); 16] = [
    (Color::Black, [0, 0, 0]),
    (Color::Red, [255, 0, 0]),
    (Color::Green, [0, 255, 0]),
    (Color::Yellow, [255, 255, 0]),
    (Color::Blue, [0, 0, 255]),
    (Color::Magenta, [255, 0, 255]),
    (Color::Cyan, [0, 255, 255]),
    (Color::Gray, [192, 192, 192]),
    (Color::DarkGray, [128, 128, 128]),
    (Color::LightRed, [255, 128, 128]),
    (Color::LightGreen, [128, 255, 128]),
    (Color::LightYellow, [255, 255, 128]),
    (Color::LightBlue, [128, 128, 255]),
    (Color::LightMagenta, [255, 128, 255]),
    (Color::LightCyan, [128, 255, 255]),
    (Color::White, [255, 255, 255]),
];

/// Nearest of the 16 standard ANSI colors.
pub(crate) fn ansi16(color: Rgba) -> Color {
    ANSI16_PALETTE
        .iter()
        .min_by_key(|(_, rgb)| channel_distance(color, *rgb))
        .map_or(Color::Reset, |(candidate, _)| *candidate)
}

fn channel_distance(color: Rgba, rgb: [u8; 3]) -> u32 {
    let delta = |from: u8, to: u8| {
        let difference = i32::from(from) - i32::from(to);
        (difference * difference) as u32
    };
    delta(color.r, rgb[0]) + delta(color.g, rgb[1]) + delta(color.b, rgb[2])
}
