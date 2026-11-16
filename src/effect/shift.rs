//! Bounded grid shifts shared by sliding effects.

use std::ops::Range;

use crate::scene::Grid;

use super::Residue;

/// Copy `source` into `grid`, shifted by `offset` and bounded to the grid.
pub(super) fn shift_all(grid: &mut Grid, source: &Grid, offset: (i32, i32), residue: Residue) {
    for y in 0..grid.rows {
        for x in 0..grid.cols {
            copy_cell(grid, source, (x, y), offset, residue);
        }
    }
}

/// Shift only the destination rows in `rows`, filling vacated cells.
pub(super) fn shift_rows(
    grid: &mut Grid,
    source: &Grid,
    rows: Range<u16>,
    offset_x: i32,
    residue: Residue,
) {
    let start = rows.start.min(grid.rows);
    let end = rows.end.min(grid.rows);
    for y in start..end {
        for x in 0..grid.cols {
            copy_cell(grid, source, (x, y), (offset_x, 0), residue);
        }
    }
}

/// Shift a single column vertically, filling vacated cells.
pub(super) fn shift_column(
    grid: &mut Grid,
    source: &Grid,
    x: u16,
    offset_y: i32,
    residue: Residue,
) {
    if x >= grid.cols {
        return;
    }
    for y in 0..grid.rows {
        copy_cell(grid, source, (x, y), (0, offset_y), residue);
    }
}

fn copy_cell(
    grid: &mut Grid,
    source: &Grid,
    position: (u16, u16),
    offset: (i32, i32),
    residue: Residue,
) {
    let (x, y) = position;
    let source_x = i64::from(x) - i64::from(offset.0);
    let source_y = i64::from(y) - i64::from(offset.1);
    let cell = match (u16::try_from(source_x), u16::try_from(source_y)) {
        (Ok(source_x), Ok(source_y)) => source
            .get(source_x, source_y)
            .copied()
            .unwrap_or_else(|| residue.cell()),
        _ => residue.cell(),
    };
    grid.set(x, y, cell);
}
