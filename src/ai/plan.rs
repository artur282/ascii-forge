//! High-level scene plan returned by an LLM or an offline template.
//!
//! The plan is deliberately small and bounded: it selects text, a template,
//! palette, font, layout and a short list of beats. Rust compiles it into a
//! [`crate::scene::Scene`]; the model never emits cell grids or raw keyframes.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::scene::{Align, Rgba};

const MAX_TITLE_CHARS: usize = 80;
const MAX_TAGLINE_CHARS: usize = 120;
const MIN_PALETTE_COLORS: usize = 2;
const MAX_PALETTE_COLORS: usize = 6;
const MIN_BEATS: usize = 1;
const MAX_BEATS: usize = 8;
const MIN_BEAT_FRAMES: u32 = 1;
const MAX_BEAT_FRAMES: u32 = 300;
const MAX_TOTAL_FRAMES: u64 = 18_000;

const MIN_CHARS_PER_FRAME: f32 = 0.01;
const MAX_CHARS_PER_FRAME: f32 = 100.0;
const MIN_OPACITY: f32 = 0.0;
const MAX_OPACITY: f32 = 1.0;
const MIN_INTENSITY: f32 = 0.0;
const MAX_INTENSITY: f32 = 1.0;
const MIN_SPEED: f32 = 0.0;
const MAX_SPEED: f32 = 20.0;
const MIN_AMPLITUDE: f32 = 0.0;
const MAX_AMPLITUDE: f32 = 100.0;
const MIN_FREQUENCY: f32 = 0.0;
const MAX_FREQUENCY: f32 = 50.0;
const MIN_PERIOD: u32 = 1;
const MAX_PERIOD: u32 = 18_000;
const MIN_OFFSET: i32 = -10_000;
const MAX_OFFSET: i32 = 10_000;

/// Template family selected by a plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TemplateKind {
    Logo,
    Promo,
    Glitch,
    Matrix,
    LowerThird,
}

/// Output aspect ratio.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Aspect {
    Landscape,
    Square,
    Portrait,
}

impl Aspect {
    /// Output dimensions for this aspect.
    pub const fn dimensions(self) -> (u32, u32) {
        match self {
            Self::Landscape => (1920, 1080),
            Self::Square => (1080, 1080),
            Self::Portrait => (1080, 1920),
        }
    }
}

/// High-level placement of the title and tagline.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Layout {
    #[default]
    Centered,
    Stacked,
    LowerThird,
    FullBleed,
}

/// Bounded effect choices available to a plan.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PlanEffect {
    None,
    Typewriter {
        #[serde(default = "default_chars_per_frame")]
        chars_per_frame: f32,
    },
    Fade {
        #[serde(default)]
        from: f32,
        #[serde(default = "default_opacity")]
        to: f32,
    },
    Slide {
        #[serde(default)]
        dx: i32,
        #[serde(default)]
        dy: i32,
    },
    Glitch {
        #[serde(default = "default_intensity")]
        intensity: f32,
    },
    Scanline {
        #[serde(default = "default_intensity")]
        intensity: f32,
        #[serde(default = "default_speed")]
        speed: f32,
    },
    Wave {
        #[serde(default = "default_amplitude")]
        amplitude: f32,
        #[serde(default = "default_frequency")]
        frequency: f32,
    },
    Dissolve,
    Blink {
        #[serde(default = "default_period")]
        period: u32,
    },
}

/// What a beat animates.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BeatTarget {
    #[default]
    Title,
    Tagline,
    Canvas,
}

/// One timed effect beat.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Beat {
    pub effect: PlanEffect,
    #[serde(default)]
    pub target: BeatTarget,
    #[serde(default = "default_beat_frames")]
    pub frames: u32,
}

/// High-level plan compiled into a scene.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ScenePlan {
    pub title: String,
    #[serde(default)]
    pub tagline: Option<String>,
    pub template: TemplateKind,
    #[serde(default = "default_aspect")]
    pub aspect: Aspect,
    #[serde(default = "default_palette")]
    pub palette: Vec<Rgba>,
    #[serde(default = "default_font")]
    pub font: String,
    #[serde(default)]
    pub layout: Layout,
    #[serde(default = "default_beats")]
    pub beats: Vec<Beat>,
    #[serde(default)]
    pub seed: u64,
    /// Optional alignment hint for the title.
    #[serde(default)]
    pub align: Align,
}

fn default_aspect() -> Aspect {
    Aspect::Landscape
}

fn default_font() -> String {
    "standard".to_string()
}

fn default_palette() -> Vec<Rgba> {
    vec![
        Rgba::rgb(10, 12, 16),
        Rgba::rgb(0, 255, 136),
        Rgba::rgb(255, 255, 255),
    ]
}

fn default_beats() -> Vec<Beat> {
    vec![Beat {
        effect: PlanEffect::Typewriter {
            chars_per_frame: 1.0,
        },
        target: BeatTarget::Title,
        frames: 30,
    }]
}

fn default_chars_per_frame() -> f32 {
    1.0
}

fn default_opacity() -> f32 {
    1.0
}

fn default_intensity() -> f32 {
    0.5
}

fn default_speed() -> f32 {
    1.0
}

fn default_amplitude() -> f32 {
    1.0
}

fn default_frequency() -> f32 {
    0.1
}

fn default_period() -> u32 {
    30
}

fn default_beat_frames() -> u32 {
    30
}

impl ScenePlan {
    /// Validate the plan against the bounds the compiler relies on.
    ///
    /// Errors are collected in document order so callers can report every
    /// problem at once. A plan that passes this check always compiles to a
    /// scene whose layers and effects fit the compiler's fixed canvas.
    pub fn validate(&self) -> Result<(), Vec<PlanValidationError>> {
        let mut errors = Vec::new();

        validate_title(self, &mut errors);
        validate_tagline(self, &mut errors);
        validate_palette(self, &mut errors);
        validate_font(self, &mut errors);
        validate_beats(self, &mut errors);

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

/// A single scene plan invariant violation.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum PlanValidationError {
    #[error("title must not be empty")]
    EmptyTitle,

    #[error("title has {len} characters; maximum is {max}")]
    TitleTooLong { len: usize, max: usize },

    #[error("tagline must not be empty when present")]
    BlankTagline,

    #[error("tagline has {len} characters; maximum is {max}")]
    TaglineTooLong { len: usize, max: usize },

    #[error("palette size {len} out of range {min}..={max}")]
    PaletteSizeOutOfRange { len: usize, min: usize, max: usize },

    #[error("unknown FIGlet font \"{font}\"; expected one of: {supported}")]
    UnknownFont { font: String, supported: String },

    #[error("plan has {len} beats; expected {min}..={max}")]
    BeatCountOutOfRange { len: usize, min: usize, max: usize },

    #[error("beat \"beat-{beat}\" has {frames} frames; expected {min}..={max}")]
    BeatFramesOutOfRange {
        beat: usize,
        frames: u32,
        min: u32,
        max: u32,
    },

    #[error("plan has {frames} total frames; maximum is {max}")]
    TotalFramesExceeded { frames: u64, max: u32 },

    #[error("beat \"beat-{beat}\" targets the tagline but the plan has no tagline")]
    TaglineTargetWithoutTagline { beat: usize },

    #[error(
        "beat \"beat-{beat}\" effect parameter {parameter} value {value} out of range {min}..={max}"
    )]
    EffectParameterOutOfRange {
        beat: usize,
        parameter: &'static str,
        value: f32,
        min: f32,
        max: f32,
    },

    #[error("beat \"beat-{beat}\" effect parameter {parameter} must be finite, got {value}")]
    EffectParameterNotFinite {
        beat: usize,
        parameter: &'static str,
        value: f32,
    },

    #[error(
        "beat \"beat-{beat}\" effect offset {parameter} value {value} out of range {min}..={max}"
    )]
    EffectOffsetOutOfRange {
        beat: usize,
        parameter: &'static str,
        value: i32,
        min: i32,
        max: i32,
    },

    #[error("beat \"beat-{beat}\" blink period {value} out of range {min}..={max}")]
    EffectPeriodOutOfRange {
        beat: usize,
        value: u32,
        min: u32,
        max: u32,
    },
}

fn validate_title(plan: &ScenePlan, errors: &mut Vec<PlanValidationError>) {
    if plan.title.trim().is_empty() {
        errors.push(PlanValidationError::EmptyTitle);
        return;
    }
    let len = plan.title.chars().count();
    if len > MAX_TITLE_CHARS {
        errors.push(PlanValidationError::TitleTooLong {
            len,
            max: MAX_TITLE_CHARS,
        });
    }
}

fn validate_tagline(plan: &ScenePlan, errors: &mut Vec<PlanValidationError>) {
    let Some(tagline) = &plan.tagline else {
        return;
    };
    if tagline.trim().is_empty() {
        errors.push(PlanValidationError::BlankTagline);
        return;
    }
    let len = tagline.chars().count();
    if len > MAX_TAGLINE_CHARS {
        errors.push(PlanValidationError::TaglineTooLong {
            len,
            max: MAX_TAGLINE_CHARS,
        });
    }
}

fn validate_palette(plan: &ScenePlan, errors: &mut Vec<PlanValidationError>) {
    let len = plan.palette.len();
    if !(MIN_PALETTE_COLORS..=MAX_PALETTE_COLORS).contains(&len) {
        errors.push(PlanValidationError::PaletteSizeOutOfRange {
            len,
            min: MIN_PALETTE_COLORS,
            max: MAX_PALETTE_COLORS,
        });
    }
}

fn validate_font(plan: &ScenePlan, errors: &mut Vec<PlanValidationError>) {
    if !crate::figlet::font_names().contains(&plan.font.as_str()) {
        errors.push(PlanValidationError::UnknownFont {
            font: plan.font.clone(),
            supported: crate::figlet::font_names().join(", "),
        });
    }
}

fn validate_beats(plan: &ScenePlan, errors: &mut Vec<PlanValidationError>) {
    let len = plan.beats.len();
    if !(MIN_BEATS..=MAX_BEATS).contains(&len) {
        errors.push(PlanValidationError::BeatCountOutOfRange {
            len,
            min: MIN_BEATS,
            max: MAX_BEATS,
        });
    }

    let tagline_present = tagline_present(plan);
    for (beat_index, beat) in plan.beats.iter().enumerate() {
        if !(MIN_BEAT_FRAMES..=MAX_BEAT_FRAMES).contains(&beat.frames) {
            errors.push(PlanValidationError::BeatFramesOutOfRange {
                beat: beat_index,
                frames: beat.frames,
                min: MIN_BEAT_FRAMES,
                max: MAX_BEAT_FRAMES,
            });
        }
        if beat.target == BeatTarget::Tagline && !tagline_present {
            errors.push(PlanValidationError::TaglineTargetWithoutTagline { beat: beat_index });
        }
        validate_effect(beat_index, beat.effect, errors);
    }

    let total: u64 = plan.beats.iter().map(|beat| u64::from(beat.frames)).sum();
    if total > MAX_TOTAL_FRAMES {
        errors.push(PlanValidationError::TotalFramesExceeded {
            frames: total,
            max: MAX_TOTAL_FRAMES as u32,
        });
    }
}

fn tagline_present(plan: &ScenePlan) -> bool {
    plan.tagline
        .as_deref()
        .is_some_and(|tagline| !tagline.trim().is_empty())
}

fn validate_effect(beat: usize, effect: PlanEffect, errors: &mut Vec<PlanValidationError>) {
    match effect {
        PlanEffect::None | PlanEffect::Dissolve => {}
        PlanEffect::Typewriter { chars_per_frame } => validate_parameter(
            beat,
            "chars_per_frame",
            chars_per_frame,
            MIN_CHARS_PER_FRAME,
            MAX_CHARS_PER_FRAME,
            errors,
        ),
        PlanEffect::Fade { from, to } => {
            validate_parameter(beat, "from", from, MIN_OPACITY, MAX_OPACITY, errors);
            validate_parameter(beat, "to", to, MIN_OPACITY, MAX_OPACITY, errors);
        }
        PlanEffect::Slide { dx, dy } => {
            validate_offset(beat, "dx", dx, errors);
            validate_offset(beat, "dy", dy, errors);
        }
        PlanEffect::Glitch { intensity } => validate_parameter(
            beat,
            "intensity",
            intensity,
            MIN_INTENSITY,
            MAX_INTENSITY,
            errors,
        ),
        PlanEffect::Scanline { intensity, speed } => {
            validate_parameter(
                beat,
                "intensity",
                intensity,
                MIN_INTENSITY,
                MAX_INTENSITY,
                errors,
            );
            validate_parameter(beat, "speed", speed, MIN_SPEED, MAX_SPEED, errors);
        }
        PlanEffect::Wave {
            amplitude,
            frequency,
        } => {
            validate_parameter(
                beat,
                "amplitude",
                amplitude,
                MIN_AMPLITUDE,
                MAX_AMPLITUDE,
                errors,
            );
            validate_parameter(
                beat,
                "frequency",
                frequency,
                MIN_FREQUENCY,
                MAX_FREQUENCY,
                errors,
            );
        }
        PlanEffect::Blink { period } => {
            if !(MIN_PERIOD..=MAX_PERIOD).contains(&period) {
                errors.push(PlanValidationError::EffectPeriodOutOfRange {
                    beat,
                    value: period,
                    min: MIN_PERIOD,
                    max: MAX_PERIOD,
                });
            }
        }
    }
}

fn validate_parameter(
    beat: usize,
    parameter: &'static str,
    value: f32,
    min: f32,
    max: f32,
    errors: &mut Vec<PlanValidationError>,
) {
    if !value.is_finite() {
        errors.push(PlanValidationError::EffectParameterNotFinite {
            beat,
            parameter,
            value,
        });
    } else if !(min..=max).contains(&value) {
        errors.push(PlanValidationError::EffectParameterOutOfRange {
            beat,
            parameter,
            value,
            min,
            max,
        });
    }
}

fn validate_offset(
    beat: usize,
    parameter: &'static str,
    value: i32,
    errors: &mut Vec<PlanValidationError>,
) {
    if !(MIN_OFFSET..=MAX_OFFSET).contains(&value) {
        errors.push(PlanValidationError::EffectOffsetOutOfRange {
            beat,
            parameter,
            value,
            min: MIN_OFFSET,
            max: MAX_OFFSET,
        });
    }
}
