//! Alternating on/off visibility.

use crate::scene::Grid;

pub(super) fn apply(grid: &mut Grid, frame: u32, period: u32) {
    if period == 0 || (frame / period).is_multiple_of(2) {
        return;
    }

    for cell in &mut grid.cells {
        cell.ch = ' ';
    }
}
