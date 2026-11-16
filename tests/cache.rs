//! Behavior tests for the project cache (spec S0).

use std::path::Path;

use ascii_forge::cache::{PROJECT_DIR, ProjectCache, sha256_hex};

#[test]
fn missing_entries_are_a_miss() {
    let dir = tempfile::tempdir().expect("temp dir");
    let cache = ProjectCache::discover(dir.path());

    assert_eq!(cache.get("ai", "missing").expect("read cache"), None);
}

#[test]
fn put_and_get_round_trip() {
    let dir = tempfile::tempdir().expect("temp dir");
    let cache = ProjectCache::discover(dir.path());

    let path = cache
        .put("ai", "abc123", br#"{"ok":true}"#)
        .expect("write cache");
    assert!(path.starts_with(dir.path().join(PROJECT_DIR)));
    assert_eq!(
        cache.get("ai", "abc123").expect("read cache"),
        Some(br#"{"ok":true}"#.to_vec())
    );
}

#[test]
fn namespaces_are_isolated() {
    let dir = tempfile::tempdir().expect("temp dir");
    let cache = ProjectCache::discover(dir.path());
    cache.put("ai", "key", b"ai").unwrap();
    cache.put("vision", "key", b"vision").unwrap();

    assert_eq!(cache.get("ai", "key").unwrap(), Some(b"ai".to_vec()));
    assert_eq!(
        cache.get("vision", "key").unwrap(),
        Some(b"vision".to_vec())
    );
}

#[test]
fn unsafe_names_and_keys_are_rejected() {
    let cache = ProjectCache::at("/tmp/ascii-forge-cache-test");

    for namespace in ["", ".", "..", "a/b", r"a\b"] {
        assert!(cache.get(namespace, "key").is_err(), "{namespace:?}");
    }
    for key in ["", ".", "..", "a/b", r"a\b"] {
        assert!(cache.get("ai", key).is_err(), "{key:?}");
    }
}

#[test]
fn artifact_paths_are_beneath_the_cache_root() {
    let cache = ProjectCache::at("/tmp/ascii-forge-cache-test");
    let path = cache
        .artifact_path(Path::new("previews/frame.png"))
        .expect("artifact path");

    assert!(path.starts_with(cache.artifact_dir()));
    assert!(cache.artifact_path(Path::new("../escape")).is_err());
    assert!(cache.artifact_path(Path::new("/absolute")).is_err());
}

#[test]
fn sha256_keys_are_stable_and_unambiguous() {
    let first = sha256_hex(&["openai", "gpt", "prompt"]);
    let second = sha256_hex(&["openai", "gpt", "prompt"]);
    assert_eq!(first, second);
    assert_eq!(first.len(), 64);
    assert_ne!(first, sha256_hex(&["openai", "gptprompt"]));
    assert_ne!(sha256_hex(&["a", "bc"]), sha256_hex(&["ab", "c"]));
}
