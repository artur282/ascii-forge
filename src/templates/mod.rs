//! Offline scene templates compiled through the same plan compiler as AI output.
//!
//! Each template is a fixed [`ScenePlan`] recipe: it chooses an aspect,
//! layout, palette, font and a short beat list, and leaves every low-level
//! decision to [`crate::ai::compile_plan`].

use crate::ai::plan::PlanValidationError;
use crate::ai::{Aspect, Beat, BeatTarget, Layout, PlanEffect, ScenePlan, TemplateKind};
use crate::scene::{Align, Rgba};
use thiserror::Error;

const DARK: Rgba = Rgba::rgb(10, 12, 16);
const BLACK: Rgba = Rgba::rgb(0, 0, 0);
const WHITE: Rgba = Rgba::rgb(255, 255, 255);
const CYAN: Rgba = Rgba::rgb(0, 255, 255);
const MAGENTA: Rgba = Rgba::rgb(255, 0, 255);
const GREEN: Rgba = Rgba::rgb(0, 255, 136);
const STANDARD_FONT: &str = "standard";
const BEAT_FRAMES: u32 = 30;

#[derive(Debug, Clone)]
pub struct TemplateParams {
    pub title: String,
    pub tagline: Option<String>,
    pub seed: u64,
}

#[derive(Debug, Error)]
pub enum TemplateError {
    #[error("template title must not be empty")]
    BlankTitle,

    #[error("invalid template plan: {}", summarize_plan_errors(.0))]
    InvalidPlan(Vec<PlanValidationError>),
}

/// Build an offline plan for `kind`.
///
/// The title is required; a blank tagline is treated as absent. The returned
/// plan has already passed [`ScenePlan::validate`], so callers can compile it
/// directly.
pub fn build(kind: TemplateKind, params: &TemplateParams) -> Result<ScenePlan, TemplateError> {
    if params.title.trim().is_empty() {
        return Err(TemplateError::BlankTitle);
    }

    let tagline = normalize_tagline(params.tagline.as_deref());
    let plan = plan_for(kind, params, tagline);

    plan.validate().map_err(TemplateError::InvalidPlan)?;
    Ok(plan)
}

fn normalize_tagline(tagline: Option<&str>) -> Option<String> {
    tagline
        .map(str::trim)
        .filter(|tagline| !tagline.is_empty())
        .map(str::to_string)
}

fn plan_for(kind: TemplateKind, params: &TemplateParams, tagline: Option<String>) -> ScenePlan {
    let has_tagline = tagline.is_some();
    let spec = TemplateSpec::for_kind(kind, has_tagline);

    ScenePlan {
        title: params.title.clone(),
        tagline,
        template: kind,
        aspect: spec.aspect,
        palette: spec.palette.to_vec(),
        font: spec.font.to_string(),
        layout: spec.layout,
        beats: spec.beats,
        seed: params.seed,
        align: Align::Center,
    }
}

/// Fixed recipe shared by [`build`] and the template tests.
struct TemplateSpec {
    aspect: Aspect,
    layout: Layout,
    font: &'static str,
    palette: [Rgba; 3],
    beats: Vec<Beat>,
}

impl TemplateSpec {
    fn for_kind(kind: TemplateKind, has_tagline: bool) -> Self {
        match kind {
            TemplateKind::Logo => Self {
                aspect: Aspect::Landscape,
                layout: Layout::Centered,
                font: STANDARD_FONT,
                palette: [DARK, CYAN, WHITE],
                beats: vec![
                    beat(
                        PlanEffect::Typewriter {
                            chars_per_frame: 1.0,
                        },
                        BeatTarget::Title,
                    ),
                    beat(PlanEffect::Fade { from: 0.0, to: 1.0 }, BeatTarget::Title),
                ],
            },
            TemplateKind::Promo => {
                let mut beats = vec![beat(PlanEffect::Slide { dx: 16, dy: 0 }, BeatTarget::Title)];
                if has_tagline {
                    beats.push(beat(
                        PlanEffect::Fade { from: 0.0, to: 1.0 },
                        BeatTarget::Tagline,
                    ));
                }
                beats.push(beat(
                    PlanEffect::Typewriter {
                        chars_per_frame: 1.0,
                    },
                    BeatTarget::Title,
                ));
                Self {
                    aspect: Aspect::Landscape,
                    layout: Layout::Stacked,
                    font: STANDARD_FONT,
                    palette: [DARK, MAGENTA, WHITE],
                    beats,
                }
            }
            TemplateKind::Glitch => Self {
                aspect: Aspect::Landscape,
                layout: Layout::FullBleed,
                font: STANDARD_FONT,
                palette: [BLACK, GREEN, GREEN],
                beats: vec![
                    beat(PlanEffect::Glitch { intensity: 0.5 }, BeatTarget::Canvas),
                    beat(
                        PlanEffect::Scanline {
                            intensity: 0.5,
                            speed: 1.0,
                        },
                        BeatTarget::Canvas,
                    ),
                ],
            },
            TemplateKind::Matrix => Self {
                aspect: Aspect::Portrait,
                layout: Layout::FullBleed,
                font: "slant",
                palette: [BLACK, GREEN, GREEN],
                beats: vec![
                    beat(PlanEffect::Dissolve, BeatTarget::Title),
                    beat(
                        PlanEffect::Scanline {
                            intensity: 0.5,
                            speed: 1.0,
                        },
                        BeatTarget::Canvas,
                    ),
                ],
            },
            TemplateKind::LowerThird => {
                let mut beats = vec![beat(PlanEffect::Slide { dx: 16, dy: 0 }, BeatTarget::Title)];
                if has_tagline {
                    beats.push(beat(
                        PlanEffect::Typewriter {
                            chars_per_frame: 1.0,
                        },
                        BeatTarget::Tagline,
                    ));
                }
                Self {
                    aspect: Aspect::Landscape,
                    layout: Layout::LowerThird,
                    font: STANDARD_FONT,
                    palette: [DARK, WHITE, CYAN],
                    beats,
                }
            }
        }
    }
}

fn beat(effect: PlanEffect, target: BeatTarget) -> Beat {
    Beat {
        effect,
        target,
        frames: BEAT_FRAMES,
    }
}

fn summarize_plan_errors(errors: &[PlanValidationError]) -> String {
    errors
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("; ")
}
