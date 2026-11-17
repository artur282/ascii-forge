//! Behavior tests for the AI provider clients and response cache (spec S2).
//!
//! Every test is offline: the mock provider stands in for the network and the
//! cache lives in a temporary project directory.

use std::fs;

use ascii_forge::ai::provider::{
    ALLOW_INSECURE_HTTP_ENV, CachedProvider, MockProvider, OpenAiProvider, PlanRequest,
    PlanResponse, Provider, ProviderError, ResponseFormat, TokenUsage, allow_insecure_http,
    extract_json, request_body, resolve_api_key, response_cache_key, validate_base_url_with,
};
use ascii_forge::cache::{PROJECT_DIR, ProjectCache};
use serde_json::json;

fn sample_request() -> PlanRequest {
    PlanRequest {
        prompt: "A neon nebula logo".to_string(),
        schema: json!({
            "type": "object",
            "properties": { "title": { "type": "string" } },
            "required": ["title"],
        }),
        system: "You are an ASCII art director.".to_string(),
        repair: None,
    }
}

fn response_with_usage(content: &str) -> PlanResponse {
    PlanResponse {
        content: content.to_string(),
        usage: Some(TokenUsage {
            prompt_tokens: 10,
            completion_tokens: 20,
            total_tokens: 30,
        }),
    }
}

#[test]
fn extract_json_accepts_a_raw_object() {
    let content = r#"{"title":"NEBULA"}"#;

    assert_eq!(extract_json(content).expect("raw json"), content);
}

#[test]
fn extract_json_strips_markdown_fences() {
    let content = "```json\n{\"title\":\"NEBULA\"}\n```";

    assert_eq!(
        extract_json(content).expect("fenced json"),
        r#"{"title":"NEBULA"}"#
    );
}

#[test]
fn extract_json_finds_an_object_embedded_in_prose() {
    let content = "Sure! Here is the plan: {\"title\":\"NEBULA\"} Enjoy.";

    assert_eq!(
        extract_json(content).expect("embedded json"),
        r#"{"title":"NEBULA"}"#
    );
}

#[test]
fn extract_json_handles_nested_objects_and_braces_in_strings() {
    let content = "Plan: {\"title\":\"A { B }\",\"beats\":[{\"frames\":30}]} done";

    assert_eq!(
        extract_json(content).expect("nested json"),
        r#"{"title":"A { B }","beats":[{"frames":30}]}"#
    );
}

#[test]
fn extract_json_rejects_empty_and_invalid_text() {
    for content in ["", "   ", "no json here", "{\"title\": ", "{ oops }", "]"] {
        let error = extract_json(content).expect_err("invalid content must fail");
        assert!(
            matches!(error, ProviderError::InvalidResponse { .. }),
            "{content:?} produced {error}"
        );
    }
}

#[test]
fn mock_provider_returns_queued_responses_and_counts_calls() {
    let mut provider = MockProvider::new();
    provider.push_response(r#"{"title":"NEBULA"}"#);
    provider.push_error(ProviderError::InvalidResponse {
        message: "plan failed validation".to_string(),
    });

    let request = sample_request();
    let first = provider.complete(&request).expect("queued response");
    assert_eq!(first.content, r#"{"title":"NEBULA"}"#);

    let second = provider.complete(&request).expect_err("queued error");
    assert_eq!(
        second,
        ProviderError::InvalidResponse {
            message: "plan failed validation".to_string(),
        }
    );

    assert!(provider.complete(&request).is_err(), "queue is drained");
    assert_eq!(provider.call_count(), 3);
    assert_eq!(provider.pending_count(), 0);
}

#[test]
fn missing_api_key_is_a_clear_error() {
    let error = resolve_api_key(|_| None).expect_err("no key configured");

    assert!(matches!(error, ProviderError::MissingApiKey));
    let message = error.to_string();
    assert!(message.contains("ASCII_FORGE_API_KEY"), "{message}");
    assert!(message.contains("OPENAI_API_KEY"), "{message}");
}

#[test]
fn api_key_resolution_prefers_the_project_variable() {
    let preferred = resolve_api_key(|name| match name {
        "ASCII_FORGE_API_KEY" => Some("project".to_string()),
        "OPENAI_API_KEY" => Some("legacy".to_string()),
        _ => None,
    })
    .expect("project key");
    assert_eq!(preferred, "project");

    let fallback = resolve_api_key(|name| (name == "OPENAI_API_KEY").then(|| "legacy".to_string()))
        .expect("fallback key");
    assert_eq!(fallback, "legacy");

    let blank_primary = resolve_api_key(|name| match name {
        "ASCII_FORGE_API_KEY" => Some("   ".to_string()),
        "OPENAI_API_KEY" => Some("legacy".to_string()),
        _ => None,
    })
    .expect("blank values are skipped");
    assert_eq!(blank_primary, "legacy");
}

#[test]
fn response_cache_key_is_stable_and_depends_on_every_part() {
    let schema = sample_request().schema;
    let key = response_cache_key("openai", "gpt-5", "NEBULA", &schema);

    assert_eq!(key.len(), 64);
    assert_eq!(
        key,
        response_cache_key("openai", "gpt-5", "NEBULA", &sample_request().schema)
    );
    assert_ne!(
        key,
        response_cache_key("deepseek", "gpt-5", "NEBULA", &schema)
    );
    assert_ne!(
        key,
        response_cache_key("openai", "gpt-6", "NEBULA", &schema)
    );
    assert_ne!(key, response_cache_key("openai", "gpt-5", "OTHER", &schema));
    assert_ne!(
        key,
        response_cache_key("openai", "gpt-5", "NEBULA", &json!({ "type": "object" }))
    );
}

#[test]
fn response_cache_key_is_insensitive_to_schema_key_order() {
    let first = json!({ "type": "object", "required": ["title"] });
    let second = json!({ "required": ["title"], "type": "object" });

    assert_eq!(
        response_cache_key("openai", "gpt-5", "NEBULA", &first),
        response_cache_key("openai", "gpt-5", "NEBULA", &second)
    );
}

#[test]
fn plan_response_round_trips_with_token_usage() {
    let response = response_with_usage(r#"{"title":"NEBULA"}"#);

    let encoded = serde_json::to_string_pretty(&response).expect("serialize response");
    let decoded: PlanResponse = serde_json::from_str(&encoded).expect("deserialize response");

    assert_eq!(decoded, response);
    assert_eq!(decoded.usage.expect("usage").total_tokens, 30);
}

#[test]
fn cached_provider_misses_call_the_inner_provider_and_write_the_cache() {
    let dir = tempfile::tempdir().expect("temp dir");
    let cache = ProjectCache::discover(dir.path());
    let inner = MockProvider::from_contents([r#"{"title":"NEBULA"}"#]);
    let provider = CachedProvider::new(inner, cache, "test-key");

    let response = provider.complete(&sample_request()).expect("complete");

    assert_eq!(response.content, r#"{"title":"NEBULA"}"#);
    assert_eq!(provider.inner().call_count(), 1);

    let path = dir
        .path()
        .join(PROJECT_DIR)
        .join("cache")
        .join("ai")
        .join("test-key.json");
    let stored = fs::read_to_string(&path).expect("cache entry written");
    assert!(
        stored.contains('\n'),
        "cache entry is stored as pretty JSON"
    );
    let decoded: PlanResponse = serde_json::from_str(&stored).expect("valid cached JSON");
    assert_eq!(decoded, response);
}

#[test]
fn cached_provider_hits_do_not_call_the_inner_provider() {
    let dir = tempfile::tempdir().expect("temp dir");
    let cache = ProjectCache::discover(dir.path());
    let inner = MockProvider::from_contents([r#"{"title":"NEBULA"}"#]);
    let provider = CachedProvider::new(inner, cache, "hit-key");

    let first = provider.complete(&sample_request()).expect("first call");
    let second = provider.complete(&sample_request()).expect("cached call");

    assert_eq!(first, second);
    assert_eq!(provider.inner().call_count(), 1);
}

#[test]
fn malformed_cache_entries_are_treated_as_misses() {
    let dir = tempfile::tempdir().expect("temp dir");
    let cache = ProjectCache::discover(dir.path());
    cache
        .put("ai", "broken-key", br#"{ not valid json"#)
        .expect("seed malformed entry");
    let inner = MockProvider::from_contents([r#"{"title":"FRESH"}"#]);
    let provider = CachedProvider::new(inner, cache, "broken-key");

    let response = provider.complete(&sample_request()).expect("complete");

    assert_eq!(response.content, r#"{"title":"FRESH"}"#);
    assert_eq!(provider.inner().call_count(), 1);

    let path = dir
        .path()
        .join(PROJECT_DIR)
        .join("cache")
        .join("ai")
        .join("broken-key.json");
    let stored = fs::read_to_string(&path).expect("cache entry repaired");
    let decoded: PlanResponse = serde_json::from_str(&stored).expect("repaired JSON");
    assert_eq!(decoded, response);
}

#[test]
fn openai_provider_requires_key_base_url_and_model() {
    let cases = [
        ("", "https://api.openai.com/v1", "gpt-5", "api key"),
        ("secret", "", "gpt-5", "base URL"),
        ("secret", "https://api.openai.com/v1", "", "model"),
    ];

    for (key, base_url, model, field) in cases {
        let error = OpenAiProvider::new(key, base_url, model)
            .expect_err("blank configuration must be rejected");
        let message = error.to_string();
        assert!(
            message.to_lowercase().contains(&field.to_lowercase()),
            "{field:?} missing from {message:?}"
        );
    }

    assert!(OpenAiProvider::new("secret", "https://api.openai.com/v1", "gpt-5").is_ok());
}

#[test]
fn openai_provider_debug_hides_the_api_key() {
    let provider = OpenAiProvider::new("super-secret-key", "https://api.openai.com/v1", "gpt-5")
        .expect("provider");

    let debug = format!("{provider:?}");

    assert!(!debug.contains("super-secret-key"), "{debug}");
    assert!(debug.contains("redacted"), "{debug}");
}

#[test]
fn request_body_uses_a_strict_json_schema() {
    let request = sample_request();

    let body = request_body("gpt-5", &request, ResponseFormat::JsonSchema);

    assert_eq!(body["model"], "gpt-5");
    assert_eq!(body["temperature"], 0.7);
    assert_eq!(body["messages"][0]["role"], "system");
    assert_eq!(body["messages"][0]["content"], request.system);
    assert_eq!(body["messages"][1]["role"], "user");
    assert_eq!(body["messages"][1]["content"], request.prompt);
    assert_eq!(body["response_format"]["type"], "json_schema");
    assert_eq!(body["response_format"]["json_schema"]["name"], "scene_plan");
    assert_eq!(body["response_format"]["json_schema"]["strict"], true);
    assert_eq!(
        body["response_format"]["json_schema"]["schema"],
        request.schema
    );
}

#[test]
fn request_body_falls_back_to_json_object_mode() {
    let body = request_body("gpt-5", &sample_request(), ResponseFormat::JsonObject);

    assert_eq!(body["response_format"], json!({ "type": "json_object" }));
}

#[test]
fn request_body_includes_the_repair_text() {
    let mut request = sample_request();
    request.repair = Some("beats[0].frames must be between 1 and 60".to_string());

    let body = request_body("gpt-5", &request, ResponseFormat::JsonSchema);
    let user = body["messages"][1]["content"]
        .as_str()
        .expect("user message is a string");

    assert!(user.contains("A neon nebula logo"), "{user}");
    assert!(
        user.contains("beats[0].frames must be between 1 and 60"),
        "{user}"
    );
}

#[test]
fn base_url_validation_allows_https_for_any_host() {
    for url in [
        "https://api.openai.com/v1",
        "https://example.com/v1",
        "https://localhost:8443/v1",
    ] {
        validate_base_url_with(url, false)
            .unwrap_or_else(|error| panic!("{url} must be allowed: {error}"));
    }
}

#[test]
fn base_url_validation_allows_plain_http_on_loopback_hosts() {
    for url in [
        "http://localhost:11434/v1",
        "http://LOCALHOST/v1",
        "http://127.0.0.1:8080/v1",
        "http://[::1]:8080/v1",
        "http://::1/v1",
    ] {
        validate_base_url_with(url, false)
            .unwrap_or_else(|error| panic!("{url} is loopback and must be allowed: {error}"));
    }
}

#[test]
fn base_url_validation_rejects_plain_http_to_remote_hosts() {
    for url in [
        "http://api.openai.com/v1",
        "http://10.0.0.1:8080/v1",
        "http://example.com",
        "http://localhost.example.com/v1",
        "http://127.0.0.1.example.com/v1",
    ] {
        let error =
            validate_base_url_with(url, false).expect_err("remote plain http must be rejected");
        assert!(
            matches!(&error, ProviderError::InsecureBaseUrl { .. }),
            "{url} produced {error:?}"
        );
        let message = error.to_string();
        assert!(message.contains("clear text"), "{url}: {message}");
        assert!(
            message.contains(ALLOW_INSECURE_HTTP_ENV),
            "{url}: {message}"
        );
    }
}

#[test]
fn base_url_validation_accepts_the_explicit_insecure_override() {
    validate_base_url_with("http://api.example.com/v1", true)
        .expect("the explicit override allows remote plain http");

    assert!(allow_insecure_http(|name| {
        (name == ALLOW_INSECURE_HTTP_ENV).then(|| "1".to_string())
    }));
    assert!(!allow_insecure_http(|name| {
        (name == ALLOW_INSECURE_HTTP_ENV).then(|| "true".to_string())
    }));
    assert!(!allow_insecure_http(|name| {
        (name == ALLOW_INSECURE_HTTP_ENV).then(|| " 0 ".to_string())
    }));
    assert!(!allow_insecure_http(|_| None));
}

#[test]
fn base_url_validation_rejects_non_http_schemes_and_empty_authorities() {
    for url in [
        "ftp://example.com/v1",
        "api.openai.com/v1",
        "https://",
        "http:///v1",
    ] {
        let error = validate_base_url_with(url, true)
            .expect_err("only absolute http(s) URLs with a host are valid");
        assert!(
            matches!(&error, ProviderError::InvalidBaseUrl { .. }),
            "{url} produced {error:?}"
        );
    }
}

#[test]
fn openai_provider_accepts_https_and_loopback_http() {
    for url in [
        "https://api.openai.com/v1",
        "http://localhost:11434/v1",
        "http://127.0.0.1:8080/v1",
        "http://[::1]:8080/v1",
    ] {
        OpenAiProvider::new("secret", url, "gpt-5")
            .unwrap_or_else(|error| panic!("{url} must build a provider: {error}"));
    }
}
