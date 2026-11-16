//! Translating slide.

use crate::scene::Grid;

use super::{Residue, shift};

pub(super) fn apply(grid: &mut Grid, progress: f32, dx: i32, dy: i32, residue: Residue) {
    let source = grid.clone();
    let offset = (scaled(progress, dx), scaled(progress, dy));
    shift::shift_all(grid, &source, offset, residue);
}

/// Rounded travel for one axis at the current progress.
fn scaled(progress: f32, delta: i32) -> i32 {
    (progress * delta as f32).round() as i32
}
