//! Generation loop tests: parsing, repair retries, cache behavior and schema.

use ascii_forge::ai::generate::{
    MAX_GENERATION_ATTEMPTS, generate_plan, generation_cache_key, scene_plan_schema, system_prompt,
};
use ascii_forge::ai::provider::{AI_CACHE_NAMESPACE, MockProvider};
use ascii_forge::ai::{ScenePlan, TemplateKind};
use ascii_forge::cache::ProjectCache;
use ascii_forge::templates::{TemplateParams, build};

fn valid_plan_json() -> String {
    let plan = build(
        TemplateKind::Logo,
        &TemplateParams {
            title: "NEBULA".to_string(),
            tagline: Some("navigate the noise".to_string()),
            seed: 7,
        },
    )
    .expect("template plan");
    serde_json::to_string(&plan).expect("plan JSON")
}

fn valid_plan() -> ScenePlan {
    serde_json::from_str(&valid_plan_json()).expect("plan")
}

#[test]
fn schema_and_prompt_describe_the_plan_contract() {
    let schema = scene_plan_schema();
    let properties = schema["properties"].as_object().expect("schema properties");
    assert!(properties.contains_key("title"));
    assert!(properties.contains_key("beats"));

    let prompt = system_prompt();
    for expected in ["logo", "promo", "glitch", "matrix", "lower_third"] {
        assert!(prompt.contains(expected), "prompt must mention {expected}");
    }
    assert!(prompt.contains("standard"));
}

#[test]
fn valid_response_compiles_on_the_first_attempt() {
    let provider = MockProvider::from_contents([valid_plan_json()]);
    let schema = scene_plan_schema();
    let generated = generate_plan(&provider, "a neon logo", &schema, None).expect("generation");

    assert!(!generated.cached);
    assert_eq!(generated.attempts, 1);
    assert_eq!(generated.plan.title, "NEBULA");
    assert_eq!(provider.call_count(), 1);
    assert!(generated.scene.validate().is_ok());
}

#[test]
fn fenced_json_is_accepted() {
    let content = format!("```json\n{}\n```", valid_plan_json());
    let provider = MockProvider::from_contents([content]);
    let schema = scene_plan_schema();
    let generated = generate_plan(&provider, "a neon logo", &schema, None).expect("generation");

    assert_eq!(generated.attempts, 1);
}

#[test]
fn invalid_content_is_repaired_within_the_budget() {
    let provider = MockProvider::from_contents(vec!["not json".to_string(), valid_plan_json()]);
    let schema = scene_plan_schema();
    let generated = generate_plan(&provider, "a neon logo", &schema, None).expect("generation");

    assert_eq!(generated.attempts, 2);
    assert_eq!(provider.call_count(), 2);
}

#[test]
fn invalid_plan_is_repaired_within_the_budget() {
    let mut plan = valid_plan();
    plan.title = String::new();
    let invalid = serde_json::to_string(&plan).expect("invalid plan JSON");
    let provider = MockProvider::from_contents([invalid, valid_plan_json()]);
    let schema = scene_plan_schema();
    let generated = generate_plan(&provider, "a neon logo", &schema, None).expect("generation");

    assert_eq!(generated.attempts, 2);
}

#[test]
fn generation_stops_after_two_repairs() {
    let provider = MockProvider::from_contents(["bad", "worse", "still bad"]);
    let schema = scene_plan_schema();
    let error = generate_plan(&provider, "a neon logo", &schema, None).expect_err("must fail");

    assert!(error.to_string().contains("after 3 attempts"), "{error}");
    assert_eq!(
        provider.call_count(),
        usize::try_from(MAX_GENERATION_ATTEMPTS).unwrap()
    );
}

#[test]
fn empty_prompt_is_rejected_before_calling_the_provider() {
    let provider = MockProvider::from_contents([valid_plan_json()]);
    let schema = scene_plan_schema();
    let error = generate_plan(&provider, "  ", &schema, None).expect_err("must fail");

    assert!(error.to_string().contains("prompt"));
    assert_eq!(provider.call_count(), 0);
}

#[test]
fn cache_hit_skips_the_provider() {
    let dir = tempfile::tempdir().expect("temp dir");
    let cache = ProjectCache::discover(dir.path());
    let schema = scene_plan_schema();
    let key = generation_cache_key("openai", "mock-model", "a neon logo", &schema);

    let first = MockProvider::from_contents([valid_plan_json()]);
    let generated =
        generate_plan(&first, "a neon logo", &schema, Some((&cache, &key))).expect("first");
    assert!(!generated.cached);
    assert_eq!(first.call_count(), 1);
    assert!(cache.get(AI_CACHE_NAMESPACE, &key).unwrap().is_some());

    let second = MockProvider::new();
    let cached =
        generate_plan(&second, "a neon logo", &schema, Some((&cache, &key))).expect("cached");
    assert!(cached.cached);
    assert_eq!(cached.attempts, 0);
    assert_eq!(second.call_count(), 0);
    assert_eq!(cached.plan, generated.plan);
}

#[test]
fn malformed_cache_entry_is_a_miss() {
    let dir = tempfile::tempdir().expect("temp dir");
    let cache = ProjectCache::discover(dir.path());
    let schema = scene_plan_schema();
    let key = generation_cache_key("openai", "mock-model", "a neon logo", &schema);
    cache
        .put(AI_CACHE_NAMESPACE, &key, b"not-json")
        .expect("write malformed cache");

    let provider = MockProvider::from_contents([valid_plan_json()]);
    let generated =
        generate_plan(&provider, "a neon logo", &schema, Some((&cache, &key))).expect("generation");

    assert!(!generated.cached);
    assert_eq!(provider.call_count(), 1);
}

#[test]
fn cache_key_changes_with_model_prompt_and_schema() {
    let schema = scene_plan_schema();
    let base = generation_cache_key("openai", "model-a", "prompt", &schema);
    assert_eq!(
        base,
        generation_cache_key("openai", "model-a", "prompt", &schema)
    );
    assert_ne!(
        base,
        generation_cache_key("openai", "model-b", "prompt", &schema)
    );
    assert_ne!(
        base,
        generation_cache_key("openai", "model-a", "other", &schema)
    );
    let other_schema = serde_json::json!({"type": "object"});
    assert_ne!(
        base,
        generation_cache_key("openai", "model-a", "prompt", &other_schema)
    );
}
