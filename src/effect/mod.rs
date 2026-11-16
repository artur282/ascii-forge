//! Deterministic, reusable effects.
//!
//! An effect is active only while `start <= frame < end`, and it is a pure
//! function of the frame, the scene seed, its parameters and the grid it
//! receives. Layer effects run on the layer grid before it is overlaid; canvas
//! effects run after every layer has been composited.

mod blink;
mod dissolve;
mod fade;
mod glitch;
mod random;
mod scanline;
mod shift;
mod slide;
mod typewriter;
mod wave;

use crate::scene::{Cell, Effect, EffectKind, Grid, Rgba};
use thiserror::Error;

/// Inputs shared by every effect.
#[derive(Debug, Clone, Copy)]
pub struct EffectContext {
    /// Scene-level seed, combined with the effect seed and id.
    pub scene_seed: u64,
    /// Canvas background used by color effects and shifted cells.
    pub background: Rgba,
}

#[derive(Debug, Error)]
pub enum EffectError {
    #[error("effect \"{effect}\" has invalid parameters: {reason}")]
    InvalidParameters { effect: String, reason: String },
}

/// Fill used where an effect vacated a cell.
///
/// Layer effects must leave transparent cells so the overlay keeps the canvas
/// underneath; canvas effects leave the scene background instead.
#[derive(Debug, Clone, Copy)]
enum Residue {
    Transparent,
    Background(Rgba),
}

impl Residue {
    fn cell(self) -> Cell {
        match self {
            Self::Transparent => Cell {
                ch: ' ',
                fg: Rgba::TRANSPARENT,
                bg: Rgba::TRANSPARENT,
            },
            Self::Background(color) => Cell {
                ch: ' ',
                fg: color,
                bg: color,
            },
        }
    }
}

/// Apply active effects that target `layer_id` to a layer grid.
pub fn apply_layer(
    effects: &[Effect],
    layer_id: &str,
    frame: u32,
    grid: &mut Grid,
    context: &EffectContext,
) -> Result<(), EffectError> {
    let selection = Selection::Layer(layer_id);
    apply_selected(
        effects,
        selection,
        frame,
        grid,
        context,
        Residue::Transparent,
    )
}

/// Apply active canvas-wide effects after all layers were composited.
pub fn apply_canvas(
    effects: &[Effect],
    frame: u32,
    grid: &mut Grid,
    context: &EffectContext,
) -> Result<(), EffectError> {
    let residue = Residue::Background(context.background);
    apply_selected(effects, Selection::Canvas, frame, grid, context, residue)
}

/// Which effects are eligible for one application pass.
#[derive(Debug, Clone, Copy)]
enum Selection<'a> {
    Layer(&'a str),
    Canvas,
}

impl Selection<'_> {
    fn matches(self, effect: &Effect) -> bool {
        match (self, effect.target.as_deref()) {
            (Self::Layer(id), Some(target)) => id == target,
            (Self::Canvas, None) => true,
            _ => false,
        }
    }
}

fn apply_selected(
    effects: &[Effect],
    selection: Selection<'_>,
    frame: u32,
    grid: &mut Grid,
    context: &EffectContext,
    residue: Residue,
) -> Result<(), EffectError> {
    for effect in effects {
        if selection.matches(effect) && is_active(effect, frame) {
            apply_effect(effect, frame, grid, context, residue)?;
        }
    }
    Ok(())
}

fn is_active(effect: &Effect, frame: u32) -> bool {
    frame >= effect.start && frame < effect.end
}

/// Normalized progress over the active window, clamped to 0..=1.
fn progress(effect: &Effect, frame: u32) -> f32 {
    let span = effect.end - effect.start;
    ((frame - effect.start) as f32 / span as f32).clamp(0.0, 1.0)
}

fn apply_effect(
    effect: &Effect,
    frame: u32,
    grid: &mut Grid,
    context: &EffectContext,
    residue: Residue,
) -> Result<(), EffectError> {
    let progress = progress(effect, frame);
    match &effect.kind {
        EffectKind::Typewriter { chars_per_frame } => {
            let chars_per_frame = positive_param(effect, "chars_per_frame", *chars_per_frame)?;
            typewriter::apply(grid, frame, effect.start, chars_per_frame);
        }
        EffectKind::Fade { from, to } => {
            let from = finite_param(effect, "from", *from)?;
            let to = finite_param(effect, "to", *to)?;
            fade::apply(grid, progress, from, to, context.background);
        }
        EffectKind::Slide { dx, dy } => {
            slide::apply(grid, progress, *dx, *dy, residue);
        }
        EffectKind::Scanline { intensity, speed } => {
            let intensity = finite_param(effect, "intensity", *intensity)?;
            let speed = finite_param(effect, "speed", *speed)?;
            scanline::apply(
                grid,
                frame,
                effect.start,
                intensity,
                speed,
                context.background,
            );
        }
        EffectKind::Glitch { intensity, seed } => {
            let intensity = finite_param(effect, "intensity", *intensity)?;
            let seed = random::combine(context.scene_seed, Some(*seed), &effect.id);
            glitch::apply(grid, frame, intensity, seed, residue);
        }
        EffectKind::Wave {
            amplitude,
            frequency,
            speed,
        } => {
            let amplitude = finite_param(effect, "amplitude", *amplitude)?;
            let frequency = finite_param(effect, "frequency", *frequency)?;
            let speed = finite_param(effect, "speed", *speed)?;
            wave::apply(grid, frame, amplitude, frequency, speed, residue);
        }
        EffectKind::Dissolve { seed } => {
            let seed = random::combine(context.scene_seed, Some(*seed), &effect.id);
            dissolve::apply(grid, progress, seed);
        }
        EffectKind::Blink { period } => blink::apply(grid, frame, *period),
    }
    Ok(())
}

fn finite_param(effect: &Effect, parameter: &str, value: f32) -> Result<f32, EffectError> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(invalid_parameters(
            effect,
            format!("{parameter} must be finite, got {value}"),
        ))
    }
}

fn positive_param(effect: &Effect, parameter: &str, value: f32) -> Result<f32, EffectError> {
    let value = finite_param(effect, parameter, value)?;
    if value > 0.0 {
        Ok(value)
    } else {
        Err(invalid_parameters(
            effect,
            format!("{parameter} must be greater than 0, got {value}"),
        ))
    }
}

fn invalid_parameters(effect: &Effect, reason: String) -> EffectError {
    EffectError::InvalidParameters {
        effect: effect.id.clone(),
        reason,
    }
}
