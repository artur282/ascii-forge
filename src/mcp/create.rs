//! `create_scene` implementation: offline templates and AI prompt mode.

use std::path::Path;

use serde_json::{Value, json};

use crate::ai::provider::{OpenAiProvider, TokenUsage, api_key_from_env};
use crate::ai::{
    GenerateError, GeneratedPlan, ScenePlan, compile_plan, generate_plan, generation_cache_key,
    scene_plan_schema,
};
use crate::cache::ProjectCache;
use crate::cli::Settings;
use crate::scene::Scene;
use crate::templates::{TemplateParams, build};

use super::error::{ToolError, redact_secret};
use super::params::{CreateSceneParams, TemplateName};
use super::paths;

/// Providers the current OpenAI-compatible client can actually reach.
const SUPPORTED_PROVIDERS: [&str; 2] = ["openai", "openai-compatible"];

/// A scene plus the generation metadata reported to the MCP client.
struct PreparedScene {
    plan: ScenePlan,
    scene: Scene,
    origin: &'static str,
    template: Option<&'static str>,
    attempts: u32,
    cached: bool,
    usage: Option<TokenUsage>,
}

/// Create and write a scene, returning the structured tool payload.
pub fn create_scene(params: CreateSceneParams) -> Result<Value, ToolError> {
    let cwd = paths::current_dir()?;
    let explicit_relative = params
        .out
        .as_deref()
        .map(|raw| paths::safe_relative(raw, "out"))
        .transpose()?;
    // Reject an existing explicit target before spending a provider call.
    if let Some(relative) = explicit_relative.as_deref() {
        let absolute = paths::prepare_output_path(&cwd, relative)?;
        paths::ensure_writable_target(&absolute, relative, params.overwrite)?;
    }

    let prepared = match (params.prompt.as_deref(), params.template) {
        (Some(prompt), None) => prepare_prompt(prompt, &params, &cwd)?,
        (None, Some(template)) => prepare_template(template, &params)?,
        (Some(_), Some(_)) => {
            return Err(ToolError::invalid_input(
                "pass either `prompt` or `template`, not both",
            ));
        }
        (None, None) => {
            return Err(ToolError::invalid_input(
                "create_scene requires a `prompt` or a `template`",
            ));
        }
    };

    let relative = match explicit_relative {
        Some(relative) => relative,
        None => paths::default_scene_relative(&prepared.plan.title, prepared.plan.seed),
    };
    let absolute = paths::prepare_output_path(&cwd, &relative)?;
    paths::ensure_writable_target(&absolute, &relative, params.overwrite)?;
    paths::ensure_parent(&absolute)?;

    let contents = serde_json::to_string_pretty(&prepared.scene)
        .map_err(|error| ToolError::io(format!("could not serialize the scene: {error}")))?;
    paths::write_atomic(&absolute, contents.as_bytes())?;

    Ok(json!({
        "scene_path": paths::relative_display(&cwd, &absolute)?,
        "origin": prepared.origin,
        "template": prepared.template,
        "frames": prepared.scene.timeline.frames,
        "seed": prepared.scene.seed,
        "attempts": prepared.attempts,
        "cached": prepared.cached,
        "usage": prepared.usage,
    }))
}

fn prepare_template(
    template: TemplateName,
    params: &CreateSceneParams,
) -> Result<PreparedScene, ToolError> {
    let title = params
        .title
        .as_deref()
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .ok_or_else(|| ToolError::invalid_input("template mode requires a non-blank `title`"))?;
    let tagline = params
        .tagline
        .as_deref()
        .map(str::trim)
        .filter(|tagline| !tagline.is_empty())
        .map(str::to_string);

    let plan = build(
        template.kind(),
        &TemplateParams {
            title: title.to_string(),
            tagline,
            seed: params.seed.unwrap_or(0),
        },
    )
    .map_err(|error| ToolError::invalid_input(error.to_string()))?;
    let scene = compile_plan(&plan).map_err(|error| ToolError::invalid_input(error.to_string()))?;

    Ok(PreparedScene {
        plan,
        scene,
        origin: "template",
        template: Some(template.as_str()),
        attempts: 0,
        cached: false,
        usage: None,
    })
}

fn prepare_prompt(
    prompt: &str,
    params: &CreateSceneParams,
    cwd: &Path,
) -> Result<PreparedScene, ToolError> {
    let prompt = prompt.trim();
    if prompt.is_empty() {
        return Err(ToolError::invalid_input("`prompt` must not be blank"));
    }

    let settings = Settings::load()
        .map_err(|error| ToolError::config(format!("could not load settings: {error:#}")))?;
    let provider_name = settings
        .config
        .ai
        .provider
        .clone()
        .unwrap_or_else(|| "openai".to_string())
        .to_lowercase();
    validate_provider_name(&provider_name)?;
    let model = settings
        .config
        .ai
        .model
        .clone()
        .unwrap_or_else(|| "gpt-4o-mini".to_string());
    let base_url = settings
        .config
        .ai
        .base_url
        .clone()
        .unwrap_or_else(|| "https://api.openai.com/v1".to_string());

    let api_key = api_key_from_env().map_err(|error| ToolError::provider(error.to_string()))?;
    let provider = OpenAiProvider::new(api_key.clone(), base_url, model.clone())
        .map_err(|error| ToolError::provider(redact_secret(error.to_string(), &api_key)))?;
    let schema = scene_plan_schema();
    let key = generation_cache_key(&provider_name, &model, prompt, &schema);
    let cache = if params.no_cache {
        None
    } else {
        Some(ProjectCache::discover(cwd))
    };

    let generated = generate_plan(
        &provider,
        prompt,
        &schema,
        cache.as_ref().map(|cache| (cache, key.as_str())),
    )
    .map_err(|error| generation_error(error, &api_key))?;
    let GeneratedPlan {
        mut plan,
        scene,
        usage,
        attempts,
        cached,
    } = generated;

    let mut changed = false;
    if let Some(title) = params.title.as_deref() {
        if title.trim().is_empty() {
            return Err(ToolError::invalid_input("`title` must not be blank"));
        }
        plan.title = title.to_string();
        changed = true;
    }
    if let Some(tagline) = params.tagline.as_deref() {
        plan.tagline = if tagline.trim().is_empty() {
            None
        } else {
            Some(tagline.to_string())
        };
        changed = true;
    }
    if let Some(seed) = params.seed {
        plan.seed = seed;
        changed = true;
    }

    let scene = if changed {
        compile_plan(&plan).map_err(|error| ToolError::invalid_input(error.to_string()))?
    } else {
        scene
    };

    Ok(PreparedScene {
        plan,
        scene,
        origin: "ai",
        template: None,
        attempts,
        cached,
        usage,
    })
}

fn validate_provider_name(provider_name: &str) -> Result<(), ToolError> {
    if SUPPORTED_PROVIDERS.contains(&provider_name) {
        return Ok(());
    }
    Err(ToolError::provider(format!(
        "unsupported AI provider `{provider_name}`: only `openai` and `openai-compatible` are supported; set ai.provider in the config"
    )))
}

fn generation_error(error: GenerateError, api_key: &str) -> ToolError {
    let message = redact_secret(error.to_string(), api_key);
    match &error {
        GenerateError::EmptyPrompt | GenerateError::Compile(_) => ToolError::invalid_input(message),
        GenerateError::Provider(_) => ToolError::provider(message),
        GenerateError::InvalidContent(_) | GenerateError::Exhausted { .. } => {
            ToolError::generation(message)
        }
    }
}
