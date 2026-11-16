//! Seeded horizontal tearing with a single color-channel offset.

use std::ops::Range;

use crate::scene::{Grid, Rgba};

use super::random::{self, SplitMix64};
use super::{Residue, shift};

/// Bands are added as `intensity * rows / BANDS_DIVISOR`.
const BANDS_DIVISOR: f32 = 4.0;
/// Maximum channel offset in 8-bit steps at full intensity.
const CHANNEL_STEP: f32 = 64.0;

pub(super) fn apply(grid: &mut Grid, frame: u32, intensity: f32, seed: u64, residue: Residue) {
    let intensity = intensity.clamp(0.0, 1.0);
    if intensity == 0.0 || grid.rows == 0 || grid.cols == 0 {
        return;
    }

    let mut rng = SplitMix64::new(random::mix64(seed ^ u64::from(frame)));
    let band_count = band_count(grid.rows, intensity);
    let max_shift = (intensity * f32::from(grid.cols)).round().max(1.0) as u32;
    let source = grid.clone();

    for band in 0..band_count {
        let rows = band_rows(grid.rows, band, band_count);
        let magnitude = rng.bounded(u64::from(max_shift) + 1) as i32;
        let offset_x = if rng.coin() { magnitude } else { -magnitude };
        if offset_x != 0 {
            shift::shift_rows(grid, &source, rows, offset_x, residue);
        }
    }

    perturb_channel(grid, intensity, band_count, &mut rng);
}

fn band_count(rows: u16, intensity: f32) -> u32 {
    let scaled = (intensity * f32::from(rows) / BANDS_DIVISOR).ceil() as u32;
    scaled.clamp(1, u32::from(rows))
}

fn band_rows(rows: u16, index: u32, count: u32) -> Range<u16> {
    let total = u32::from(rows);
    let start = (u64::from(index) * u64::from(total) / u64::from(count)) as u16;
    let end = (u64::from(index + 1) * u64::from(total) / u64::from(count)) as u16;
    start..end
}

/// Shift at most one color channel inside one deterministic band.
fn perturb_channel(grid: &mut Grid, intensity: f32, bands: u32, rng: &mut SplitMix64) {
    let delta = (intensity * CHANNEL_STEP).round() as i32;
    if delta == 0 {
        return;
    }

    let channel = rng.bounded(3) as u8;
    let band = rng.bounded(u64::from(bands)) as u32;
    let delta = if rng.coin() { delta } else { -delta };

    for y in band_rows(grid.rows, band, bands) {
        for x in 0..grid.cols {
            let Some(cell) = grid.get_mut(x, y) else {
                continue;
            };
            adjust_channel(&mut cell.fg, channel, delta);
        }
    }
}

fn adjust_channel(color: &mut Rgba, channel: u8, delta: i32) {
    let adjust = |value: u8| (i32::from(value) + delta).clamp(0, 255) as u8;
    match channel {
        0 => color.r = adjust(color.r),
        1 => color.g = adjust(color.g),
        _ => color.b = adjust(color.b),
    }
}
