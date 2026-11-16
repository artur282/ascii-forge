use ascii_forge::scene::Scene;

#[test]
fn scene_json_schema_is_generated() {
    let schema = schemars::schema_for!(Scene);
    let json = serde_json::to_value(&schema).expect("schema must serialize to JSON");

    assert_eq!(
        json.get("title").and_then(|value| value.as_str()),
        Some("Scene")
    );
    assert!(
        json.get("properties").is_some() || json.get("$ref").is_some(),
        "schema must describe the Scene object: {json}"
    );
    assert!(
        json.get("$defs").is_some() || json.get("definitions").is_some(),
        "schema must include definitions for nested types: {json}"
    );
}
