//! Deterministic scene composition.
//!
//! A frame is built by rendering each layer, applying its layer-targeted
//! effects, transforming and overlaying it, and finally applying canvas-wide
//! effects. Timeline interpolation supplies the transform for every frame.

use crate::effect::{self, EffectContext};
use crate::figlet::{self, FigletOptions};
use crate::scene::{Canvas, Cell, Frame, Grid, Layer, LayerKind, Rgba, Scene};
use crate::timeline;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ComposeError {
    #[error("frame {frame} is outside the timeline (0..{frames})")]
    FrameOutOfRange { frame: u32, frames: u32 },

    #[error("layer \"{layer}\" is not supported for rendering: {kind}")]
    UnsupportedLayer { layer: String, kind: &'static str },

    #[error("layer \"{layer}\" failed to render: {source}")]
    Figlet {
        layer: String,
        #[source]
        source: figlet::FigletError,
    },

    #[error("effect failed: {0}")]
    Effect(#[from] effect::EffectError),
}

/// Compose one frame from the scene's layers, keyframes and effects.
pub fn compose_frame(scene: &Scene, frame_index: u32) -> Result<Frame, ComposeError> {
    if frame_index >= scene.timeline.frames {
        return Err(ComposeError::FrameOutOfRange {
            frame: frame_index,
            frames: scene.timeline.frames,
        });
    }

    let canvas = &scene.canvas;
    let context = EffectContext {
        scene_seed: scene.seed,
        background: canvas.background,
    };
    let mut grid = Grid::blank(
        canvas.cols,
        canvas.rows,
        canvas.foreground,
        canvas.background,
    );

    for layer in &scene.layers {
        let transform = timeline::transform_at(layer, frame_index);
        let mut layer_grid = render_layer(layer, canvas)?;
        if (transform.scale - 1.0).abs() > f32::EPSILON {
            layer_grid = scale_grid(&layer_grid, transform.scale);
        }
        effect::apply_layer(
            &scene.effects,
            &layer.id,
            frame_index,
            &mut layer_grid,
            &context,
        )?;
        overlay(
            &mut grid,
            &layer_grid,
            transform.x,
            transform.y,
            transform.opacity,
            canvas.background,
        );
    }

    effect::apply_canvas(&scene.effects, frame_index, &mut grid, &context)?;

    Ok(Frame {
        index: frame_index,
        grid,
    })
}

fn render_layer(layer: &Layer, canvas: &Canvas) -> Result<Grid, ComposeError> {
    Ok(match &layer.kind {
        LayerKind::Text(text) => {
            let options = FigletOptions {
                font: "standard".to_string(),
                color: Some(text.color.unwrap_or(canvas.foreground)),
                max_cols: canvas.cols,
                max_rows: canvas.rows,
                align: text.align,
            };
            figlet::render_text(&text.text, &options).map_err(|source| ComposeError::Figlet {
                layer: layer.id.clone(),
                source,
            })?
        }
        LayerKind::Figlet(figlet_layer) => {
            let options = FigletOptions {
                font: figlet_layer.font.clone(),
                color: Some(figlet_layer.color.unwrap_or(canvas.foreground)),
                max_cols: canvas.cols,
                max_rows: canvas.rows,
                align: crate::scene::Align::Center,
            };
            figlet::render_figlet(&figlet_layer.text, &options).map_err(|source| {
                ComposeError::Figlet {
                    layer: layer.id.clone(),
                    source,
                }
            })?
        }
        LayerKind::Grid(grid_layer) => grid_layer.grid.clone(),
        LayerKind::Image(_) => {
            return Err(ComposeError::UnsupportedLayer {
                layer: layer.id.clone(),
                kind: "image",
            });
        }
    })
}

/// Nearest-neighbor scale of a layer grid.
fn scale_grid(grid: &Grid, scale: f32) -> Grid {
    if !scale.is_finite() || scale <= 0.0 {
        return grid.clone();
    }
    let cols = ((f32::from(grid.cols) * scale).round() as u32).clamp(1, u32::from(u16::MAX)) as u16;
    let rows = ((f32::from(grid.rows) * scale).round() as u32).clamp(1, u32::from(u16::MAX)) as u16;
    let mut scaled = Grid::blank(cols, rows, Rgba::TRANSPARENT, Rgba::TRANSPARENT);

    for y in 0..rows {
        let source_y = ((f32::from(y) / scale).floor() as u16).min(grid.rows.saturating_sub(1));
        for x in 0..cols {
            let source_x = ((f32::from(x) / scale).floor() as u16).min(grid.cols.saturating_sub(1));
            if let Some(cell) = grid.get(source_x, source_y) {
                scaled.set(x, y, *cell);
            }
        }
    }
    scaled
}

/// Paste `source` onto `target` at `(offset_x, offset_y)`, clipping to bounds.
///
/// Spaces with a transparent background do not erase what is underneath, which
/// keeps FIGlet and text layers from punching holes in the canvas. Opacity
/// blends a glyph toward the destination background.
fn overlay(
    target: &mut Grid,
    source: &Grid,
    offset_x: i32,
    offset_y: i32,
    opacity: f32,
    background: Rgba,
) {
    let opacity = opacity.clamp(0.0, 1.0);
    if opacity <= 0.0 {
        return;
    }
    for source_y in 0..source.rows {
        for source_x in 0..source.cols {
            let Some(cell) = source.get(source_x, source_y) else {
                continue;
            };
            let target_x = i64::from(source_x) + i64::from(offset_x);
            let target_y = i64::from(source_y) + i64::from(offset_y);
            if target_x < 0 || target_y < 0 {
                continue;
            }
            let Ok(target_x) = u16::try_from(target_x) else {
                continue;
            };
            let Ok(target_y) = u16::try_from(target_y) else {
                continue;
            };
            let Some(destination) = target.get_mut(target_x, target_y) else {
                continue;
            };

            if cell.bg.a != 0 {
                destination.bg = destination.bg.blend(cell.bg, opacity);
            }
            if cell.ch != ' ' {
                let backdrop = if destination.bg.a == 0 {
                    background
                } else {
                    destination.bg
                };
                destination.ch = cell.ch;
                destination.fg = cell.fg.blend(backdrop, 1.0 - opacity);
            }
        }
    }
}

/// Fill a canvas with a single color, used by tests and future exporters.
pub fn solid_grid(cols: u16, rows: u16, foreground: Rgba, background: Rgba) -> Grid {
    Grid::new(
        cols,
        rows,
        Cell {
            ch: ' ',
            fg: foreground,
            bg: background,
        },
    )
}
