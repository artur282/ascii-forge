//! `render_preview` implementation: a bounded text or ANSI frame preview.

use serde_json::{Value, json};

use crate::render::raster;
use crate::scene::compose::compose_frame;
use crate::scene::{Frame, Grid, Rgba};

use super::error::ToolError;
use super::params::{PreviewFormat, RenderPreviewParams};
use super::paths;

const DEFAULT_MAX_COLS: u16 = 120;
const DEFAULT_MAX_ROWS: u16 = 30;
const MAX_MAX_COLS: u16 = 500;
const MAX_MAX_ROWS: u16 = 200;

/// Compose one frame and return at most `max_cols` by `max_rows` cells.
pub fn render_preview(params: RenderPreviewParams) -> Result<Value, ToolError> {
    let cwd = paths::current_dir()?;
    let (scene_file, scene) = paths::load_scene(&cwd, &params.scene_path)?;

    let frame_index = params.frame.unwrap_or(0);
    if frame_index >= scene.timeline.frames {
        return Err(ToolError::invalid_input(format!(
            "frame {frame_index} is outside the timeline 0..{}",
            scene.timeline.frames
        )));
    }
    let max_cols = bounded(
        params.max_cols.unwrap_or(DEFAULT_MAX_COLS),
        MAX_MAX_COLS,
        "max_cols",
    )?;
    let max_rows = bounded(
        params.max_rows.unwrap_or(DEFAULT_MAX_ROWS),
        MAX_MAX_ROWS,
        "max_rows",
    )?;
    let format = params.format.unwrap_or_default();

    let frame = compose_frame(&scene, frame_index).map_err(|error| {
        ToolError::render(format!("could not compose frame {frame_index}: {error}"))
    })?;
    let cols = frame.grid.cols.min(max_cols);
    let rows = frame.grid.rows.min(max_rows);
    let cropped = crop(
        &frame,
        cols,
        rows,
        scene.canvas.foreground,
        scene.canvas.background,
    );
    let preview = match format {
        PreviewFormat::Text => raster::frame_to_text(&cropped),
        PreviewFormat::Ansi => raster::frame_to_ansi(&cropped, true),
    };

    Ok(json!({
        "scene_path": paths::relative_display(&cwd, &scene_file)?,
        "format": format.as_str(),
        "frame": frame_index,
        "frames": scene.timeline.frames,
        "cols": cols,
        "rows": rows,
        "truncated": cols < frame.grid.cols || rows < frame.grid.rows,
        "preview": preview,
    }))
}

/// The `cols` and `rows` bounds are explicit: an out-of-range request is an
/// error, never an unbounded document.
fn bounded(value: u16, max: u16, name: &str) -> Result<u16, ToolError> {
    if value == 0 {
        return Err(ToolError::invalid_input(format!(
            "{name} must be at least 1"
        )));
    }
    if value > max {
        return Err(ToolError::invalid_input(format!(
            "{name} must be at most {max}, got {value}"
        )));
    }
    Ok(value)
}

fn crop(frame: &Frame, cols: u16, rows: u16, foreground: Rgba, background: Rgba) -> Frame {
    let mut grid = Grid::blank(cols, rows, foreground, background);
    for y in 0..rows {
        for x in 0..cols {
            if let Some(cell) = frame.grid.get(x, y) {
                grid.set(x, y, *cell);
            }
        }
    }
    Frame {
        index: frame.index,
        grid,
    }
}
