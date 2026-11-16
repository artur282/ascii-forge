//! Per-column vertical sine wave.

use crate::scene::Grid;

use super::{Residue, shift};

/// Divisor that converts frame speed into radians of travel.
const SPEED_DIVISOR: f32 = 10.0;

pub(super) fn apply(
    grid: &mut Grid,
    frame: u32,
    amplitude: f32,
    frequency: f32,
    speed: f32,
    residue: Residue,
) {
    let source = grid.clone();
    let frame_phase = speed.max(0.0) * frame as f32 / SPEED_DIVISOR;

    for x in 0..grid.cols {
        let angle = frequency * f32::from(x) + frame_phase;
        let offset_y = (amplitude * angle.sin()).round() as i32;
        shift::shift_column(grid, &source, x, offset_y, residue);
    }
}
