//! OpenAI-compatible provider clients and the offline mock provider.
//!
//! The provider is deliberately narrow: it receives a prompt, a JSON schema and
//! an optional repair note, and returns raw model text. Parsing the plan and
//! compiling it stays in Rust. API keys are only held in memory and are never
//! logged or forwarded to child processes.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::fmt;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use thiserror::Error;

use crate::cache::{ProjectCache, sha256_hex};

/// Cache namespace used for provider responses.
pub const AI_CACHE_NAMESPACE: &str = "ai";

/// Environment variables checked for an API key, in precedence order.
pub const API_KEY_ENV_VARS: [&str; 2] = ["ASCII_FORGE_API_KEY", "OPENAI_API_KEY"];

/// Set to `1` to allow plain http provider URLs for trusted non-loopback hosts.
pub const ALLOW_INSECURE_HTTP_ENV: &str = "ASCII_FORGE_ALLOW_INSECURE_HTTP";

const SCENE_PLAN_SCHEMA_NAME: &str = "scene_plan";
const DEFAULT_TEMPERATURE: f64 = 0.7;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);
const MAX_ERROR_BODY_CHARS: usize = 2000;

/// Token usage reported by a provider.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenUsage {
    #[serde(default)]
    pub prompt_tokens: u64,
    #[serde(default)]
    pub completion_tokens: u64,
    #[serde(default)]
    pub total_tokens: u64,
}

/// One completion request for a [`crate::ai::ScenePlan`].
#[derive(Debug, Clone, PartialEq)]
pub struct PlanRequest {
    /// Natural-language scene description.
    pub prompt: String,
    /// JSON Schema the model response must satisfy.
    pub schema: Value,
    /// System instruction that fixes the model's role.
    pub system: String,
    /// Validation or parse errors from a previous attempt.
    pub repair: Option<String>,
}

/// Raw model content plus optional token usage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanResponse {
    pub content: String,
    #[serde(default)]
    pub usage: Option<TokenUsage>,
}

/// Everything that can go wrong before a plan reaches the compiler.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ProviderError {
    /// No key was found in the supported environment variables.
    #[error("missing API key: set ASCII_FORGE_API_KEY or OPENAI_API_KEY")]
    MissingApiKey,

    /// A provider setting was blank.
    #[error("invalid provider configuration: `{field}` must not be empty")]
    EmptyField { field: &'static str },

    /// The base URL is not an absolute HTTP(S) endpoint.
    #[error("invalid provider base URL `{url}`: expected an absolute http(s) URL")]
    InvalidBaseUrl { url: String },

    /// The base URL would send the API key over an unencrypted connection.
    #[error(
        "insecure provider base URL `{url}`: plain http would send the API key in clear text; \
         use https or set ASCII_FORGE_ALLOW_INSECURE_HTTP=1 for a trusted endpoint"
    )]
    InsecureBaseUrl { url: String },

    /// The provider answered with an error status.
    #[error("provider returned HTTP {status}: {body}")]
    Http { status: u16, body: String },

    /// The request never produced a usable HTTP response.
    #[error("provider transport error: {message}")]
    Transport { message: String },

    /// The response envelope or content is missing or malformed.
    #[error("invalid provider response: {message}")]
    InvalidResponse { message: String },
}

/// Anything that can turn a [`PlanRequest`] into model text.
pub trait Provider {
    fn complete(&self, request: &PlanRequest) -> Result<PlanResponse, ProviderError>;
}

/// Response format requested from an OpenAI-compatible provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseFormat {
    /// Structured output constrained by the supplied JSON Schema.
    JsonSchema,
    /// Generic JSON mode used when a provider rejects `json_schema`.
    JsonObject,
}

/// Resolve an API key from the supported environment variables.
///
/// Precedence is [`ASCII_FORGE_API_KEY`](API_KEY_ENV_VARS) then
/// [`OPENAI_API_KEY`](API_KEY_ENV_VARS); blank values are ignored.
pub fn api_key_from_env() -> Result<String, ProviderError> {
    resolve_api_key(|name| std::env::var(name).ok())
}

/// Resolve an API key through a caller-supplied environment lookup.
///
/// This is the testable core of [`api_key_from_env`]; it never reads or mutates
/// process state.
pub fn resolve_api_key(lookup: impl Fn(&str) -> Option<String>) -> Result<String, ProviderError> {
    for name in API_KEY_ENV_VARS {
        if let Some(value) = lookup(name) {
            let value = value.trim();
            if !value.is_empty() {
                return Ok(value.to_string());
            }
        }
    }
    Err(ProviderError::MissingApiKey)
}

/// Read the plain-http override from the process environment.
pub fn allow_insecure_http_from_env() -> bool {
    allow_insecure_http(|name| std::env::var(name).ok())
}

/// Testable core of [`allow_insecure_http_from_env`]; only `1` enables it.
pub fn allow_insecure_http(lookup: impl Fn(&str) -> Option<String>) -> bool {
    lookup(ALLOW_INSECURE_HTTP_ENV).is_some_and(|value| value.trim() == "1")
}

/// Validate a base URL with the process environment's insecure-http policy.
pub fn validate_base_url(base_url: &str) -> Result<(), ProviderError> {
    validate_base_url_with(base_url, allow_insecure_http_from_env())
}

/// Testable base URL policy.
///
/// HTTPS is accepted for every host. Plain http is accepted only for loopback
/// hosts, or for any host when `allow_insecure` is explicitly set.
pub fn validate_base_url_with(base_url: &str, allow_insecure: bool) -> Result<(), ProviderError> {
    let Some((scheme, authority)) = split_scheme_authority(base_url) else {
        return Err(ProviderError::InvalidBaseUrl {
            url: base_url.to_string(),
        });
    };

    if scheme.eq_ignore_ascii_case("https") {
        return Ok(());
    }
    if !scheme.eq_ignore_ascii_case("http") {
        return Err(ProviderError::InvalidBaseUrl {
            url: base_url.to_string(),
        });
    }
    if allow_insecure || is_loopback_authority(authority) {
        return Ok(());
    }
    Err(ProviderError::InsecureBaseUrl {
        url: base_url.to_string(),
    })
}

/// Extract the first balanced JSON object from model content.
///
/// Raw JSON, fenced Markdown and objects embedded in prose are all accepted.
/// Braces inside quoted strings do not affect balancing. Content without a
/// complete, parseable object is rejected.
pub fn extract_json(content: &str) -> Result<String, ProviderError> {
    if content.trim().is_empty() {
        return Err(ProviderError::InvalidResponse {
            message: "provider content is empty".to_string(),
        });
    }

    let mut depth = 0usize;
    let mut begin = None;
    let mut in_string = false;
    let mut escaped = false;

    for (index, character) in content.char_indices() {
        if begin.is_none() {
            if character == '{' {
                begin = Some(index);
                depth = 1;
            }
            continue;
        }

        if in_string {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_string = false;
            }
            continue;
        }

        match character {
            '"' => in_string = true,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    let start = begin.expect("start is set");
                    let object = &content[start..=index];
                    serde_json::from_str::<Value>(object).map_err(|error| {
                        ProviderError::InvalidResponse {
                            message: format!("provider content is not valid JSON: {error}"),
                        }
                    })?;
                    return Ok(object.to_string());
                }
            }
            _ => {}
        }
    }

    Err(ProviderError::InvalidResponse {
        message: "no complete JSON object found in provider content".to_string(),
    })
}

/// Stable cache key for a provider response.
///
/// The schema is canonicalized with sorted object keys, so semantically equal
/// schemas hash to the same key.
pub fn response_cache_key(provider: &str, model: &str, prompt: &str, schema: &Value) -> String {
    let schema = canonical_json(schema);
    sha256_hex(&["ai-response-v1", provider, model, prompt, &schema])
}

/// Build the JSON body for an OpenAI-compatible `/chat/completions` request.
///
/// Exposed so request shaping can be tested without a network.
pub fn request_body(model: &str, request: &PlanRequest, format: ResponseFormat) -> Value {
    json!({
        "model": model,
        "messages": [
            { "role": "system", "content": request.system },
            { "role": "user", "content": user_message(request) },
        ],
        "temperature": DEFAULT_TEMPERATURE,
        "response_format": response_format(format, &request.schema),
    })
}

/// OpenAI-compatible HTTP provider.
pub struct OpenAiProvider {
    api_key: String,
    base_url: String,
    model: String,
    agent: ureq::Agent,
}

impl OpenAiProvider {
    /// Build a provider for an OpenAI-compatible endpoint.
    ///
    /// The API key only lives in memory; it is never logged.
    pub fn new(
        api_key: impl Into<String>,
        base_url: impl Into<String>,
        model: impl Into<String>,
    ) -> Result<Self, ProviderError> {
        let api_key = require_non_empty(api_key.into(), "api key")?;
        let base_url = require_non_empty(base_url.into(), "base URL")?;
        let model = require_non_empty(model.into(), "model")?;

        validate_base_url(&base_url)?;

        let agent = ureq::AgentBuilder::new()
            .timeout_connect(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            .build();

        Ok(Self {
            api_key,
            base_url: base_url.trim_end_matches('/').to_string(),
            model,
            agent,
        })
    }

    fn endpoint(&self) -> String {
        format!("{}/chat/completions", self.base_url)
    }

    fn complete_with(
        &self,
        request: &PlanRequest,
        format: ResponseFormat,
    ) -> Result<PlanResponse, ProviderError> {
        let body = request_body(&self.model, request, format);
        let content = self.send(&body)?;
        parse_response(&content)
    }

    fn send(&self, body: &Value) -> Result<String, ProviderError> {
        let response = self
            .agent
            .post(&self.endpoint())
            .set("Authorization", &format!("Bearer {}", self.api_key))
            .set("Content-Type", "application/json")
            .send_json(body);

        match response {
            Ok(response) => response
                .into_string()
                .map_err(|error| ProviderError::Transport {
                    message: error.to_string(),
                }),
            Err(ureq::Error::Status(status, response)) => Err(ProviderError::Http {
                status,
                body: truncate_body(&response.into_string().unwrap_or_default()),
            }),
            Err(ureq::Error::Transport(error)) => Err(ProviderError::Transport {
                message: error.to_string(),
            }),
        }
    }
}

impl fmt::Debug for OpenAiProvider {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OpenAiProvider")
            .field("api_key", &"<redacted>")
            .field("base_url", &self.base_url)
            .field("model", &self.model)
            .finish()
    }
}

impl Provider for OpenAiProvider {
    fn complete(&self, request: &PlanRequest) -> Result<PlanResponse, ProviderError> {
        match self.complete_with(request, ResponseFormat::JsonSchema) {
            Err(ProviderError::Http {
                status: 400 | 422, ..
            }) => self.complete_with(request, ResponseFormat::JsonObject),
            outcome => outcome,
        }
    }
}

/// Provider wrapper that reads and writes [`PlanResponse`] JSON in the project
/// cache under `.ascii-forge/cache/ai/<key>.json`.
///
/// A missing or malformed entry is a cache miss; `--no-cache` is handled by the
/// caller not wrapping the provider at all.
#[derive(Debug)]
pub struct CachedProvider<P> {
    inner: P,
    cache: ProjectCache,
    key: String,
}

impl<P: Provider> CachedProvider<P> {
    /// Wrap `inner` with a response cache entry identified by `key`.
    pub fn new(inner: P, cache: ProjectCache, key: impl Into<String>) -> Self {
        Self {
            inner,
            cache,
            key: key.into(),
        }
    }

    /// The wrapped provider.
    pub fn inner(&self) -> &P {
        &self.inner
    }

    /// Cache key used for this provider.
    pub fn key(&self) -> &str {
        &self.key
    }

    fn read_cached(&self) -> Option<PlanResponse> {
        let bytes = self
            .cache
            .get(AI_CACHE_NAMESPACE, &self.key)
            .ok()
            .flatten()?;
        serde_json::from_slice(&bytes).ok()
    }

    fn store(&self, response: &PlanResponse) {
        let payload = match serde_json::to_vec_pretty(response) {
            Ok(payload) => payload,
            Err(error) => {
                tracing::warn!(%error, "failed to serialize provider response for cache");
                return;
            }
        };
        if let Err(error) = self.cache.put(AI_CACHE_NAMESPACE, &self.key, &payload) {
            tracing::warn!(%error, "failed to write provider response cache");
        }
    }
}

impl<P: Provider> Provider for CachedProvider<P> {
    fn complete(&self, request: &PlanRequest) -> Result<PlanResponse, ProviderError> {
        if let Some(cached) = self.read_cached() {
            return Ok(cached);
        }
        let response = self.inner.complete(request)?;
        self.store(&response);
        Ok(response)
    }
}

/// Offline provider backed by a deterministic queue of responses and errors.
#[derive(Debug, Default)]
pub struct MockProvider {
    outcomes: RefCell<VecDeque<MockOutcome>>,
    calls: Cell<usize>,
}

#[derive(Debug)]
enum MockOutcome {
    Response(PlanResponse),
    Error(ProviderError),
}

impl MockProvider {
    /// Create a provider with an empty queue.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a provider that returns the given contents in order.
    pub fn from_contents<I, S>(contents: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut provider = Self::new();
        for content in contents {
            provider.push_response(content);
        }
        provider
    }

    /// Queue a successful response.
    pub fn push_response(&mut self, content: impl Into<String>) -> &mut Self {
        self.outcomes
            .borrow_mut()
            .push_back(MockOutcome::Response(PlanResponse {
                content: content.into(),
                usage: None,
            }));
        self
    }

    /// Queue an error.
    pub fn push_error(&mut self, error: ProviderError) -> &mut Self {
        self.outcomes
            .borrow_mut()
            .push_back(MockOutcome::Error(error));
        self
    }

    /// Number of calls made so far.
    pub fn call_count(&self) -> usize {
        self.calls.get()
    }

    /// Number of queued outcomes not yet consumed.
    pub fn pending_count(&self) -> usize {
        self.outcomes.borrow().len()
    }
}

impl Provider for MockProvider {
    fn complete(&self, _request: &PlanRequest) -> Result<PlanResponse, ProviderError> {
        self.calls.set(self.calls.get() + 1);
        match self.outcomes.borrow_mut().pop_front() {
            Some(MockOutcome::Response(response)) => Ok(response),
            Some(MockOutcome::Error(error)) => Err(error),
            None => Err(ProviderError::InvalidResponse {
                message: "mock provider has no queued response".to_string(),
            }),
        }
    }
}

fn require_non_empty(value: String, field: &'static str) -> Result<String, ProviderError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        Err(ProviderError::EmptyField { field })
    } else {
        Ok(trimmed.to_string())
    }
}

fn split_scheme_authority(value: &str) -> Option<(&str, &str)> {
    let (scheme, rest) = value.split_once("://")?;
    let authority = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    if authority.is_empty() {
        return None;
    }
    Some((scheme, authority))
}

fn is_loopback_authority(authority: &str) -> bool {
    let host_port = authority.rsplit('@').next().unwrap_or(authority);
    let host = if let Some(rest) = host_port.strip_prefix('[') {
        let Some(end) = rest.find(']') else {
            return false;
        };
        let tail = &host_port[end + 2..];
        if !tail.is_empty() && !tail.starts_with(':') {
            return false;
        }
        &host_port[..end + 2]
    } else if host_port == "::1" || host_port.starts_with("::1:") {
        "::1"
    } else {
        host_port.split(':').next().unwrap_or(host_port)
    };
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    matches!(host.as_str(), "localhost" | "127.0.0.1" | "[::1]" | "::1")
}

fn user_message(request: &PlanRequest) -> String {
    let repair = request
        .repair
        .as_deref()
        .map(str::trim)
        .filter(|repair| !repair.is_empty());

    match repair {
        Some(repair) => format!(
            "{}\n\nThe previous plan was rejected:\n{repair}\n\nReturn a corrected JSON object that satisfies the schema.",
            request.prompt
        ),
        None => request.prompt.clone(),
    }
}

fn response_format(format: ResponseFormat, schema: &Value) -> Value {
    match format {
        ResponseFormat::JsonSchema => json!({
            "type": "json_schema",
            "json_schema": {
                "name": SCENE_PLAN_SCHEMA_NAME,
                "strict": true,
                "schema": schema,
            },
        }),
        ResponseFormat::JsonObject => json!({ "type": "json_object" }),
    }
}

#[derive(Debug, Deserialize)]
struct ChatEnvelope {
    #[serde(default)]
    choices: Vec<ChatChoice>,
    #[serde(default)]
    usage: Option<RawUsage>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatMessage,
}

#[derive(Debug, Deserialize)]
struct ChatMessage {
    #[serde(default)]
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawUsage {
    #[serde(default)]
    prompt_tokens: u64,
    #[serde(default)]
    completion_tokens: u64,
    #[serde(default)]
    total_tokens: u64,
}

impl From<RawUsage> for TokenUsage {
    fn from(raw: RawUsage) -> Self {
        Self {
            prompt_tokens: raw.prompt_tokens,
            completion_tokens: raw.completion_tokens,
            total_tokens: raw.total_tokens,
        }
    }
}

fn parse_response(body: &str) -> Result<PlanResponse, ProviderError> {
    let envelope: ChatEnvelope =
        serde_json::from_str(body).map_err(|error| ProviderError::InvalidResponse {
            message: format!("provider envelope is not valid JSON: {error}"),
        })?;

    let choice =
        envelope
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| ProviderError::InvalidResponse {
                message: "provider envelope has no choices".to_string(),
            })?;

    let content = choice
        .message
        .content
        .ok_or_else(|| ProviderError::InvalidResponse {
            message: "provider choice has no message content".to_string(),
        })?;

    Ok(PlanResponse {
        content,
        usage: envelope.usage.map(TokenUsage::from),
    })
}

fn truncate_body(body: &str) -> String {
    let mut truncated: String = body.chars().take(MAX_ERROR_BODY_CHARS).collect();
    if body.chars().count() > MAX_ERROR_BODY_CHARS {
        truncated.push('…');
    }
    truncated
}

fn canonical_json(value: &Value) -> String {
    match value {
        Value::Object(map) => {
            let mut entries: Vec<(&String, &Value)> = map.iter().collect();
            entries.sort_by_key(|(key, _)| key.as_str());
            let fields: Vec<String> = entries
                .into_iter()
                .map(|(key, value)| {
                    format!("{}:{}", Value::String(key.clone()), canonical_json(value))
                })
                .collect();
            format!("{{{}}}", fields.join(","))
        }
        Value::Array(items) => {
            let entries: Vec<String> = items.iter().map(canonical_json).collect();
            format!("[{}]", entries.join(","))
        }
        scalar => scalar.to_string(),
    }
}
