//! Seeded per-cell reveal.

use crate::scene::Grid;

use super::random;

pub(super) fn apply(grid: &mut Grid, progress: f32, seed: u64) {
    let progress = progress.clamp(0.0, 1.0);
    if progress >= 1.0 {
        return;
    }

    for (index, cell) in grid.cells.iter_mut().enumerate() {
        // The hash depends on the seed and the cell index only: the same cell
        // reveals at the same threshold on every frame, so the pattern never
        // flickers when the frame advances.
        let hidden = progress <= 0.0 || normalized_hash(seed, index) > f64::from(progress);
        if hidden {
            cell.ch = ' ';
        }
    }
}

fn normalized_hash(seed: u64, index: usize) -> f64 {
    random::cell_hash(seed, index) as f64 / u64::MAX as f64
}
