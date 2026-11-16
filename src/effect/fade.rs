//! Opacity blend toward the scene background.

use crate::scene::{Grid, Rgba};

pub(super) fn apply(grid: &mut Grid, progress: f32, from: f32, to: f32, background: Rgba) {
    let opacity = (from + (to - from) * progress).clamp(0.0, 1.0);
    let fade = 1.0 - opacity;

    for cell in &mut grid.cells {
        cell.fg = cell.fg.blend(background, fade);
        if cell.bg.a != 0 {
            cell.bg = cell.bg.blend(background, fade);
        }
    }
}
