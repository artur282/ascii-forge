//! Progressive character reveal.

use crate::scene::Grid;

/// Reveal non-space cells in row-major order; unrevealed cells become spaces
/// while keeping their background.
pub(super) fn apply(grid: &mut Grid, frame: u32, start: u32, chars_per_frame: f32) {
    let elapsed = u64::from(frame.saturating_sub(start)) + 1;
    let reveal = (elapsed as f64 * f64::from(chars_per_frame)).ceil() as usize;
    let mut revealed = 0usize;

    for cell in &mut grid.cells {
        if cell.ch == ' ' {
            continue;
        }
        if revealed >= reveal {
            cell.ch = ' ';
        }
        revealed += 1;
    }
}
