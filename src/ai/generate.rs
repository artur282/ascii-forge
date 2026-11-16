//! Plan generation loop: provider call, defensive JSON parsing, bounded repair
//! retries, optional cache and deterministic compilation.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::ai::compile::{CompileError, compile_plan};
use crate::ai::plan::ScenePlan;
use crate::ai::provider::{
    AI_CACHE_NAMESPACE, PlanRequest, Provider, ProviderError, TokenUsage, extract_json,
    response_cache_key,
};
use crate::cache::ProjectCache;
use crate::figlet::font_names;
use crate::scene::Scene;

/// Initial attempt plus at most two repair attempts.
pub const MAX_GENERATION_ATTEMPTS: u32 = 3;

#[derive(Debug, Error)]
pub enum GenerateError {
    #[error("prompt must not be empty")]
    EmptyPrompt,

    #[error(transparent)]
    Provider(#[from] ProviderError),

    #[error("model response is not usable: {0}")]
    InvalidContent(String),

    #[error("generation failed after {attempts} attempts: {errors}")]
    Exhausted { attempts: u32, errors: String },

    #[error(transparent)]
    Compile(#[from] CompileError),
}

/// Cache payload for a successful generation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedGeneration {
    pub plan: ScenePlan,
    #[serde(default)]
    pub usage: Option<TokenUsage>,
}

/// A validated plan and its deterministic scene.
#[derive(Debug, Clone)]
pub struct GeneratedPlan {
    pub plan: ScenePlan,
    pub scene: Scene,
    pub usage: Option<TokenUsage>,
    /// 1-based provider attempts; 0 means the cache supplied the plan.
    pub attempts: u32,
    pub cached: bool,
}

/// JSON Schema sent to the provider.
pub fn scene_plan_schema() -> Value {
    serde_json::to_value(schemars::schema_for!(ScenePlan)).expect("ScenePlan schema serializes")
}

/// System instruction for the art-director role.
pub fn system_prompt() -> String {
    let fonts = font_names().join(", ");
    format!(
        "You are ASCII Forge's art director. Return only one JSON object matching the \
         ScenePlan schema. Choose a title, optional tagline, template, aspect, palette, \
         font, layout, 1-8 beats and a seed. Do not return ASCII grids, cell coordinates, \
         raw keyframes, file paths, shell commands, code or pixels; Rust compiles the plan. \
         Available fonts: {fonts}. Templates: logo, promo, glitch, matrix, lower_third. \
         Aspects: landscape, square, portrait. Layouts: centered, stacked, lower_third, \
         full_bleed. Effects: none, typewriter, fade, slide, glitch, scanline, wave, \
         dissolve, blink. Palette: 2-6 RGBA colors. Title <= 80 characters, tagline <= 120. \
         Each beat lasts 1-300 frames; use the seed for deterministic variety."
    )
}

/// Generate a plan with an optional project cache.
///
/// Provider errors are fatal; parse, validation and compile failures trigger at
/// most two repair attempts with the error text in the next request.
pub fn generate_plan<P: Provider>(
    provider: &P,
    prompt: &str,
    schema: &Value,
    cache: Option<(&ProjectCache, &str)>,
) -> Result<GeneratedPlan, GenerateError> {
    if prompt.trim().is_empty() {
        return Err(GenerateError::EmptyPrompt);
    }

    if let Some((cache, key)) = cache
        && let Some(cached) = read_cached(cache, key)
        && let Ok(scene) = compile_plan(&cached.plan)
    {
        return Ok(GeneratedPlan {
            plan: cached.plan,
            scene,
            usage: cached.usage,
            attempts: 0,
            cached: true,
        });
    }

    let mut failures = Vec::new();
    let mut repair = None;

    for attempt in 1..=MAX_GENERATION_ATTEMPTS {
        let request = PlanRequest {
            prompt: prompt.to_string(),
            schema: schema.clone(),
            system: system_prompt(),
            repair: repair.clone(),
        };
        let response = provider.complete(&request)?;

        match parse_plan(&response.content) {
            Ok(plan) => match compile_plan(&plan) {
                Ok(scene) => {
                    if let Some((cache, key)) = cache {
                        write_cached(cache, key, &plan, response.usage);
                    }
                    return Ok(GeneratedPlan {
                        plan,
                        scene,
                        usage: response.usage,
                        attempts: attempt,
                        cached: false,
                    });
                }
                Err(error) => {
                    let message = format!("attempt {attempt}: {error}");
                    repair = Some(repair_prompt(&message));
                    failures.push(message);
                }
            },
            Err(error) => {
                let message = format!("attempt {attempt}: {error}");
                repair = Some(repair_prompt(&message));
                failures.push(message);
            }
        }
    }

    Err(GenerateError::Exhausted {
        attempts: MAX_GENERATION_ATTEMPTS,
        errors: failures.join("; "),
    })
}

/// Stable cache key for one provider/model/prompt/schema combination.
pub fn generation_cache_key(provider: &str, model: &str, prompt: &str, schema: &Value) -> String {
    response_cache_key(provider, model, prompt, schema)
}

fn parse_plan(content: &str) -> Result<ScenePlan, String> {
    let json = extract_json(content).map_err(|error| error.to_string())?;
    serde_json::from_str(&json).map_err(|error| format!("invalid ScenePlan JSON: {error}"))
}

fn repair_prompt(error: &str) -> String {
    format!(
        "The previous response was rejected: {error}. Return only one corrected JSON object \
         matching the ScenePlan schema. Do not add prose, fences or explanations."
    )
}

fn read_cached(cache: &ProjectCache, key: &str) -> Option<CachedGeneration> {
    match cache.get(AI_CACHE_NAMESPACE, key) {
        Ok(Some(bytes)) => serde_json::from_slice(&bytes).ok(),
        Ok(None) => None,
        Err(error) => {
            tracing::warn!(%error, "ignoring unreadable generation cache");
            None
        }
    }
}

fn write_cached(cache: &ProjectCache, key: &str, plan: &ScenePlan, usage: Option<TokenUsage>) {
    let payload = CachedGeneration {
        plan: plan.clone(),
        usage,
    };
    let Ok(bytes) = serde_json::to_vec_pretty(&payload) else {
        return;
    };
    if let Err(error) = cache.put(AI_CACHE_NAMESPACE, key, &bytes) {
        tracing::warn!(%error, "could not write generation cache");
    }
}
