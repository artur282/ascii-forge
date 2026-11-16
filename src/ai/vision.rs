//! Optional vision review loop.
//!
//! One bounded pass: render the first, middle and last frames of a scene as
//! low-resolution PNGs, ask a vision-capable provider for a critique, and
//! return at most one revised [`ScenePlan`]. The caller validates and compiles
//! the revision; this module never compiles, validates or retries, and a
//! render or provider failure leaves the original scene untouched.
//!
//! Critiques are cached under the project cache's `vision` namespace using a
//! stable key over the plan JSON and, when the provider exposes one, its model
//! name. The cache is a convenience: an unreadable entry is a miss and a cache
//! failure never prevents the provider call.

use std::fmt;
use std::io::Cursor;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use image::{ImageFormat, RgbaImage};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use thiserror::Error;

use crate::ai::ScenePlan;
use crate::cache::{CacheError, ProjectCache, sha256_hex};
use crate::render::RenderOptions;
use crate::render::raster::frame_to_rgba;
use crate::scene::Scene;
use crate::scene::compose::compose_frame;

/// Cache namespace for vision critiques.
pub const VISION_NAMESPACE: &str = "vision";

/// Number of frames reviewed in one pass.
pub const REVIEW_FRAME_COUNT: usize = 3;

/// Width of each review frame in pixels.
pub const REVIEW_WIDTH: u32 = 320;

/// Height of each review frame in pixels.
pub const REVIEW_HEIGHT: u32 = 180;

/// Timeout for one vision request.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);

/// Cache-key schema version; bump it when the critique prompt changes.
const CACHE_KEY_VERSION: &str = "v1";

const CRITIQUE_PROMPT: &str = "You are an art director reviewing an ASCII animation. \
You receive three low-resolution frames: the first, the middle and the last. \
Judge legibility, contrast and balance, and propose at most one revised scene plan.\n\n\
Current scene plan (JSON):\n";

const CRITIQUE_INSTRUCTIONS: &str = "\n\n\
Reply with strict JSON only, without markdown fences, shaped exactly like:\n\
{\"revised_plan\": <full ScenePlan or null>, \"notes\": \"<short critique>\"}\n\n\
ScenePlan fields: title, tagline (optional), template (logo|promo|glitch|matrix|lower_third), \
aspect (landscape|square|portrait), palette (RGBA colors), font, layout \
(centered|stacked|lower_third|full_bleed), beats (1..=8) and seed. \
Use null for revised_plan when the frames already look good.";

/// Errors raised by the vision review loop.
#[derive(Debug, Error)]
pub enum VisionError {
    #[error("could not render review frames: {0}")]
    Render(String),

    #[error("vision provider failed: {0}")]
    Provider(String),

    #[error("vision provider returned a malformed critique: {0}")]
    MalformedCritique(String),

    #[error("vision cache failed: {0}")]
    Cache(String),

    #[error("invalid vision configuration: {0}")]
    Config(String),
}

impl From<CacheError> for VisionError {
    fn from(error: CacheError) -> Self {
        Self::Cache(error.to_string())
    }
}

/// Everything a vision provider needs for one bounded critique.
#[derive(Debug, Clone, PartialEq)]
pub struct VisionRequest {
    /// Plan that produced the reviewed scene.
    pub plan: ScenePlan,
    /// PNG bytes for the first, middle and last frames, in that order.
    pub frames: Vec<Vec<u8>>,
}

/// Provider answer: at most one revised plan plus short notes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VisionCritique {
    /// Optional single revision; the caller validates and compiles it.
    #[serde(default)]
    pub revised_plan: Option<ScenePlan>,
    /// Short human-readable critique.
    #[serde(default)]
    pub notes: String,
}

/// A vision-capable model that critiques rendered frames.
pub trait VisionProvider {
    /// Critique one request. The loop calls this at most once per review.
    fn critique(&self, request: &VisionRequest) -> Result<VisionCritique, VisionError>;

    /// Model identifier mixed into the critique cache key, when known.
    fn model(&self) -> Option<&str> {
        None
    }
}

/// Deterministic offline provider used by tests.
#[derive(Debug)]
pub struct MockVisionProvider {
    outcome: MockOutcome,
    calls: AtomicUsize,
}

#[derive(Debug)]
enum MockOutcome {
    Critique(VisionCritique),
    Error(String),
}

impl MockVisionProvider {
    /// Return `critique` on every call and count the calls.
    pub fn new(critique: VisionCritique) -> Self {
        Self {
            outcome: MockOutcome::Critique(critique),
            calls: AtomicUsize::new(0),
        }
    }

    /// Fail every call with a provider error carrying `message`.
    pub fn with_error(message: impl Into<String>) -> Self {
        Self {
            outcome: MockOutcome::Error(message.into()),
            calls: AtomicUsize::new(0),
        }
    }

    /// Number of `critique` calls observed so far.
    pub fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl VisionProvider for MockVisionProvider {
    fn critique(&self, _request: &VisionRequest) -> Result<VisionCritique, VisionError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        match &self.outcome {
            MockOutcome::Critique(critique) => Ok(critique.clone()),
            MockOutcome::Error(message) => Err(VisionError::Provider(message.clone())),
        }
    }
}

/// OpenAI-compatible chat-completions provider with image data URLs.
pub struct OpenAiVisionProvider {
    api_key: String,
    base_url: String,
    model: String,
    agent: ureq::Agent,
}

impl OpenAiVisionProvider {
    /// Build a provider, rejecting empty or scheme-less configuration.
    pub fn new(
        api_key: impl Into<String>,
        base_url: impl Into<String>,
        model: impl Into<String>,
    ) -> Result<Self, VisionError> {
        let api_key = require_non_empty("API key", api_key.into())?;
        let model = require_non_empty("model", model.into())?;
        let base_url = normalize_base_url(&base_url.into())?;

        Ok(Self {
            api_key,
            base_url,
            model,
            agent: ureq::AgentBuilder::new().timeout(REQUEST_TIMEOUT).build(),
        })
    }

    fn endpoint(&self) -> String {
        format!("{}/chat/completions", self.base_url)
    }
}

impl VisionProvider for OpenAiVisionProvider {
    fn critique(&self, request: &VisionRequest) -> Result<VisionCritique, VisionError> {
        let body = build_request_body(&self.model, request)?;
        let payload = serde_json::to_vec(&body).map_err(|error| {
            VisionError::Provider(format!("could not encode the vision request: {error}"))
        })?;

        let response = self
            .agent
            .post(&self.endpoint())
            .set("Authorization", &format!("Bearer {}", self.api_key))
            .set("Content-Type", "application/json")
            .send_bytes(&payload)
            .map_err(provider_failure)?;

        let text = response.into_string().map_err(|error| {
            VisionError::Provider(format!("could not read the vision response: {error}"))
        })?;
        parse_chat_response(&text)
    }

    fn model(&self) -> Option<&str> {
        Some(&self.model)
    }
}

/// Redact the API key from anything that formats the provider.
impl fmt::Debug for OpenAiVisionProvider {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OpenAiVisionProvider")
            .field("base_url", &self.base_url)
            .field("model", &self.model)
            .field("api_key", &"<redacted>")
            .finish()
    }
}

/// Render the first, middle and last frames as low-resolution PNG bytes.
///
/// The frames are composed with [`compose_frame`] and rasterized at
/// [`REVIEW_WIDTH`] x [`REVIEW_HEIGHT`]; everything stays in memory.
pub fn render_review_frames(scene: &Scene) -> Result<Vec<Vec<u8>>, VisionError> {
    let last = scene.timeline.frames.saturating_sub(1);
    let indices: [u32; REVIEW_FRAME_COUNT] = [0, scene.timeline.frames / 2, last];
    let options = RenderOptions {
        target_width: Some(REVIEW_WIDTH),
        target_height: Some(REVIEW_HEIGHT),
        ..RenderOptions::default()
    };

    indices
        .iter()
        .map(|&index| {
            let frame = compose_frame(scene, index).map_err(|error| {
                VisionError::Render(format!("could not compose review frame {index}: {error}"))
            })?;
            let image = frame_to_rgba(&frame, &options).map_err(|error| {
                VisionError::Render(format!("could not rasterize review frame {index}: {error}"))
            })?;
            encode_png(&image).map_err(|error| {
                VisionError::Render(format!("could not encode review frame {index}: {error}"))
            })
        })
        .collect()
}

/// Build the OpenAI-compatible request body for one critique.
///
/// The body carries a text prompt plus one `image_url` data URL per frame, so
/// callers can inspect it without touching the network.
pub fn build_request_body(model: &str, request: &VisionRequest) -> Result<Value, VisionError> {
    let plan_json = serde_json::to_string(&request.plan).map_err(|error| {
        VisionError::Provider(format!("could not serialize the scene plan: {error}"))
    })?;
    let prompt = format!("{CRITIQUE_PROMPT}{plan_json}{CRITIQUE_INSTRUCTIONS}");

    let mut content = Vec::with_capacity(request.frames.len() + 1);
    content.push(json!({ "type": "text", "text": prompt }));
    for png in &request.frames {
        content.push(json!({
            "type": "image_url",
            "image_url": {
                "url": format!("data:image/png;base64,{}", BASE64.encode(png)),
            },
        }));
    }

    Ok(json!({
        "model": model,
        "messages": [{ "role": "user", "content": content }],
        "temperature": 0,
    }))
}

/// Stable cache key over the plan JSON and the optional model name.
pub fn critique_cache_key(plan: &ScenePlan, model: Option<&str>) -> Result<String, VisionError> {
    let plan_json = serde_json::to_string(plan).map_err(|error| {
        VisionError::Cache(format!(
            "could not serialize the plan for the cache key: {error}"
        ))
    })?;
    Ok(sha256_hex(&[
        "vision-critique",
        CACHE_KEY_VERSION,
        &plan_json,
        model.unwrap_or_default(),
    ]))
}

/// Run one bounded vision review.
///
/// The frames are rendered first, then a cached critique is used when present.
/// Otherwise the provider is called exactly once and its critique is cached on
/// a best-effort basis. Returns the provider's revised plan, if any; it is not
/// validated or compiled here.
pub fn review_once(
    plan: &ScenePlan,
    scene: &Scene,
    provider: &dyn VisionProvider,
    cache: Option<&ProjectCache>,
) -> Result<Option<ScenePlan>, VisionError> {
    let frames = render_review_frames(scene)?;
    let request = VisionRequest {
        plan: plan.clone(),
        frames,
    };

    let cache_key = match critique_cache_key(plan, provider.model()) {
        Ok(key) => Some(key),
        Err(error) => {
            tracing::warn!(%error, "vision critique will not be cached");
            None
        }
    };

    if let (Some(cache), Some(key)) = (cache, cache_key.as_deref()) {
        match read_cached_critique(cache, key) {
            Ok(Some(critique)) => return Ok(critique.revised_plan),
            Ok(None) => {}
            Err(error) => tracing::warn!(%error, "ignoring unreadable vision cache entry"),
        }
    }

    let critique = provider.critique(&request)?;

    if let (Some(cache), Some(key)) = (cache, cache_key.as_deref())
        && let Err(error) = store_critique(cache, key, &critique)
    {
        tracing::warn!(%error, "could not cache the vision critique");
    }

    Ok(critique.revised_plan)
}

/// Parse a critique payload, tolerating one markdown code fence.
pub fn parse_critique(content: &str) -> Result<VisionCritique, VisionError> {
    serde_json::from_str(strip_code_fences(content)).map_err(|error| {
        VisionError::MalformedCritique(format!("could not parse the vision critique: {error}"))
    })
}

fn require_non_empty(field: &str, value: String) -> Result<String, VisionError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(VisionError::Config(format!(
            "vision {field} must not be empty"
        )));
    }
    Ok(trimmed.to_string())
}

fn normalize_base_url(value: &str) -> Result<String, VisionError> {
    let trimmed = value.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err(VisionError::Config(
            "vision base URL must not be empty".to_string(),
        ));
    }
    if !(trimmed.starts_with("http://") || trimmed.starts_with("https://")) {
        return Err(VisionError::Config(format!(
            "vision base URL must start with http:// or https://, got `{trimmed}`"
        )));
    }
    Ok(trimmed.to_string())
}

fn encode_png(image: &RgbaImage) -> Result<Vec<u8>, String> {
    let mut buffer = Cursor::new(Vec::new());
    image
        .write_to(&mut buffer, ImageFormat::Png)
        .map_err(|error| error.to_string())?;
    Ok(buffer.into_inner())
}

fn provider_failure(error: ureq::Error) -> VisionError {
    match error {
        ureq::Error::Status(status, response) => {
            let detail = response.into_string().unwrap_or_default();
            let detail: String = detail.trim().chars().take(200).collect();
            VisionError::Provider(format!("vision endpoint returned HTTP {status}: {detail}"))
        }
        ureq::Error::Transport(transport) => {
            VisionError::Provider(format!("vision request failed: {transport}"))
        }
    }
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatMessage,
}

#[derive(Deserialize)]
struct ChatMessage {
    content: Option<String>,
}

fn parse_chat_response(body: &str) -> Result<VisionCritique, VisionError> {
    let response: ChatResponse = serde_json::from_str(body).map_err(|error| {
        VisionError::Provider(format!("unexpected vision response body: {error}"))
    })?;
    let content = response
        .choices
        .into_iter()
        .next()
        .and_then(|choice| choice.message.content)
        .ok_or_else(|| {
            VisionError::MalformedCritique("the vision response has no message content".to_string())
        })?;
    parse_critique(&content)
}

fn strip_code_fences(content: &str) -> &str {
    let trimmed = content.trim();
    let Some(fenced) = trimmed.strip_prefix("```") else {
        return trimmed;
    };
    let body = fenced.split_once('\n').map_or(fenced, |(_, body)| body);
    body.trim_end()
        .strip_suffix("```")
        .map_or_else(|| body.trim(), str::trim)
}

fn read_cached_critique(
    cache: &ProjectCache,
    key: &str,
) -> Result<Option<VisionCritique>, VisionError> {
    let Some(bytes) = cache.get(VISION_NAMESPACE, key)? else {
        return Ok(None);
    };
    serde_json::from_slice(&bytes).map(Some).map_err(|error| {
        VisionError::Cache(format!(
            "cached critique `{key}` is not valid JSON: {error}"
        ))
    })
}

fn store_critique(
    cache: &ProjectCache,
    key: &str,
    critique: &VisionCritique,
) -> Result<(), VisionError> {
    let bytes = serde_json::to_vec(critique).map_err(|error| {
        VisionError::Cache(format!("could not serialize the critique: {error}"))
    })?;
    cache.put(VISION_NAMESPACE, key, &bytes)?;
    Ok(())
}
