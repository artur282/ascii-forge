//! Moving horizontal darkening band.

use crate::scene::{Grid, Rgba};

/// Rows from the band center that still receive some darkening.
const BAND_HALF_WIDTH: f32 = 2.0;

pub(super) fn apply(
    grid: &mut Grid,
    frame: u32,
    start: u32,
    intensity: f32,
    speed: f32,
    background: Rgba,
) {
    let intensity = intensity.clamp(0.0, 1.0);
    if grid.rows == 0 || grid.cols == 0 || intensity == 0.0 {
        return;
    }

    let rows = f32::from(grid.rows);
    let travel = frame.saturating_sub(start) as f32 * speed.max(0.0);
    let center = travel % rows;

    for y in 0..grid.rows {
        let distance = (f32::from(y) - center).abs();
        let falloff = (1.0 - distance / BAND_HALF_WIDTH).clamp(0.0, 1.0);
        if falloff == 0.0 {
            continue;
        }
        let amount = intensity * falloff;
        for x in 0..grid.cols {
            let Some(cell) = grid.get_mut(x, y) else {
                continue;
            };
            cell.fg = cell.fg.blend(background, amount);
            if cell.bg.a != 0 {
                cell.bg = cell.bg.blend(background, amount);
            }
        }
    }
}
