//! Deterministic `ScenePlan` to `Scene` compiler.
//!
//! The compiler owns every low-level decision: the fixed character canvas,
//! layer ids, layout offsets, effect ranges and output format. A plan only
//! chooses text, palette, font, layout, aspect and bounded effect beats.

use crate::ai::plan::PlanValidationError;
use crate::ai::{BeatTarget, Layout, PlanEffect, ScenePlan};
use crate::scene::{
    Canvas, Effect, EffectKind, FigletLayer, Layer, LayerKind, OutputFormat, OutputSpec,
    SCENE_VERSION, Scene, TextLayer, Timeline, Transform, ValidationError,
};
use thiserror::Error;

const CANVAS_COLS: u16 = 80;
const CANVAS_ROWS: u16 = 24;
const CELL_ASPECT: f32 = 0.542;
const FPS: u16 = 30;
const WAVE_SPEED: f32 = 1.0;

#[derive(Debug, Error)]
pub enum CompileError {
    #[error("invalid scene plan: {}", summarize_plan_errors(.0))]
    InvalidPlan(Vec<PlanValidationError>),

    #[error("compiled scene is invalid: {}", summarize_scene_errors(.0))]
    InvalidScene(Vec<ValidationError>),
}

/// Compile a validated plan into a scene.
///
/// The plan is validated first, so a malformed palette or font can never
/// panic the layout code. The resulting scene is validated again before it is
/// returned; any violation is a compiler bug and is reported as
/// [`CompileError::InvalidScene`].
pub fn compile_plan(plan: &ScenePlan) -> Result<Scene, CompileError> {
    plan.validate().map_err(CompileError::InvalidPlan)?;

    let scene = build_scene(plan);

    scene.validate().map_err(CompileError::InvalidScene)?;
    Ok(scene)
}

fn build_scene(plan: &ScenePlan) -> Scene {
    let background = plan.palette[0];
    let foreground = plan.palette[1];
    let accent = plan.palette.get(2).copied().unwrap_or(foreground);

    let (title_y, tagline_y) = layout_offsets(plan.layout);
    let mut layers = vec![Layer {
        id: "title".to_string(),
        kind: LayerKind::Figlet(FigletLayer {
            text: plan.title.clone(),
            font: plan.font.clone(),
            color: Some(foreground),
        }),
        transform: Transform {
            x: 0,
            y: title_y,
            ..Transform::default()
        },
        keyframes: Vec::new(),
    }];

    if let Some(tagline) = &plan.tagline {
        layers.push(Layer {
            id: "tagline".to_string(),
            kind: LayerKind::Text(TextLayer {
                text: tagline.clone(),
                color: Some(accent),
                align: plan.align,
            }),
            transform: Transform {
                x: 0,
                y: tagline_y,
                ..Transform::default()
            },
            keyframes: Vec::new(),
        });
    }

    let frames: u32 = plan.beats.iter().map(|beat| beat.frames).sum();
    let (width, height) = plan.aspect.dimensions();
    let format = if frames > 1 {
        OutputFormat::Gif
    } else {
        OutputFormat::Png
    };

    Scene {
        version: SCENE_VERSION,
        seed: plan.seed,
        canvas: Canvas {
            cols: CANVAS_COLS,
            rows: CANVAS_ROWS,
            cell_aspect: CELL_ASPECT,
            background,
            foreground,
        },
        layers,
        timeline: Timeline {
            fps: FPS,
            frames,
            loop_: true,
        },
        effects: build_effects(plan),
        output: OutputSpec {
            width,
            height,
            format,
        },
    }
}

fn layout_offsets(layout: Layout) -> (i32, i32) {
    match layout {
        Layout::Centered => (4, 12),
        Layout::Stacked => (2, 10),
        Layout::LowerThird => (15, 21),
        Layout::FullBleed => (6, 16),
    }
}

fn build_effects(plan: &ScenePlan) -> Vec<Effect> {
    let mut effects = Vec::new();
    let mut start = 0_u32;

    for (index, beat) in plan.beats.iter().enumerate() {
        let end = start + beat.frames;
        if let Some(kind) = scene_effect_kind(plan, beat.effect) {
            effects.push(Effect {
                id: format!("beat-{index}"),
                kind,
                target: match beat.target {
                    BeatTarget::Title => Some("title".to_string()),
                    BeatTarget::Tagline => Some("tagline".to_string()),
                    BeatTarget::Canvas => None,
                },
                start,
                end,
            });
        }
        start = end;
    }

    effects
}

fn scene_effect_kind(plan: &ScenePlan, effect: PlanEffect) -> Option<EffectKind> {
    Some(match effect {
        PlanEffect::None => return None,
        PlanEffect::Typewriter { chars_per_frame } => EffectKind::Typewriter { chars_per_frame },
        PlanEffect::Fade { from, to } => EffectKind::Fade { from, to },
        PlanEffect::Slide { dx, dy } => EffectKind::Slide { dx, dy },
        PlanEffect::Glitch { intensity } => EffectKind::Glitch {
            intensity,
            seed: plan.seed,
        },
        PlanEffect::Scanline { intensity, speed } => EffectKind::Scanline { intensity, speed },
        PlanEffect::Wave {
            amplitude,
            frequency,
        } => EffectKind::Wave {
            amplitude,
            frequency,
            speed: WAVE_SPEED,
        },
        PlanEffect::Dissolve => EffectKind::Dissolve { seed: plan.seed },
        PlanEffect::Blink { period } => EffectKind::Blink { period },
    })
}

fn summarize_plan_errors(errors: &[PlanValidationError]) -> String {
    errors
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("; ")
}

fn summarize_scene_errors(errors: &[ValidationError]) -> String {
    errors
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("; ")
}
