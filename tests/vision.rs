//! Behavior tests for the optional vision review loop (spec S4).

use ascii_forge::ai::vision::{
    MockVisionProvider, OpenAiVisionProvider, VisionCritique, VisionError, VisionRequest,
    build_request_body, critique_cache_key, parse_critique, render_review_frames, review_once,
};
use ascii_forge::ai::{Aspect, Beat, BeatTarget, Layout, PlanEffect, ScenePlan, TemplateKind};
use ascii_forge::cache::ProjectCache;
use ascii_forge::scene::{
    Canvas, Cell, Grid, GridLayer, Layer, LayerKind, OutputSpec, Rgba, SCENE_VERSION, Scene,
    Timeline, Transform,
};
use base64::Engine as _;

/// A validated-looking plan; the vision module never compiles or validates it.
fn test_plan() -> ScenePlan {
    ScenePlan {
        title: "Hello".to_string(),
        tagline: Some("vision review".to_string()),
        template: TemplateKind::Logo,
        aspect: Aspect::Landscape,
        palette: vec![Rgba::rgb(10, 12, 16), Rgba::rgb(0, 255, 136)],
        font: "standard".to_string(),
        layout: Layout::Centered,
        beats: vec![Beat {
            effect: PlanEffect::Fade { from: 0.0, to: 1.0 },
            target: BeatTarget::Title,
            frames: 30,
        }],
        seed: 42,
        align: Default::default(),
    }
}

/// A scene with one grid layer and no FIGlet or image dependency.
fn test_scene() -> Scene {
    let foreground = Rgba::rgb(0, 255, 136);
    let background = Rgba::rgb(10, 12, 16);
    let mut grid = Grid::blank(40, 20, foreground, background);
    for (offset, ch) in "VISION".chars().enumerate() {
        grid.set(
            6 + offset as u16,
            9,
            Cell {
                ch,
                fg: foreground,
                bg: background,
            },
        );
    }

    Scene {
        version: SCENE_VERSION,
        seed: 7,
        canvas: Canvas {
            cols: 40,
            rows: 20,
            cell_aspect: 0.542,
            background,
            foreground,
        },
        layers: vec![Layer {
            id: "canvas".to_string(),
            kind: LayerKind::Grid(GridLayer { grid }),
            transform: Transform::default(),
            keyframes: Vec::new(),
        }],
        timeline: Timeline {
            fps: 30,
            frames: 12,
            loop_: true,
        },
        effects: Vec::new(),
        output: OutputSpec::default(),
    }
}

#[test]
fn render_review_frames_returns_three_decodable_pngs() {
    let frames = render_review_frames(&test_scene()).expect("review frames");

    assert_eq!(frames.len(), 3);
    for (index, bytes) in frames.iter().enumerate() {
        assert!(!bytes.is_empty(), "frame {index} must not be empty");
        let image = image::load_from_memory(bytes)
            .unwrap_or_else(|error| panic!("frame {index} is not decodable: {error}"));
        assert_eq!(image.width(), 320, "frame {index} width");
        assert_eq!(image.height(), 180, "frame {index} height");
    }
}

#[test]
fn review_once_returns_the_provider_revision() {
    let plan = test_plan();
    let revised = ScenePlan {
        title: "Hello, again".to_string(),
        ..plan.clone()
    };
    let provider = MockVisionProvider::new(VisionCritique {
        revised_plan: Some(revised.clone()),
        notes: "tighten the title".to_string(),
    });

    let result = review_once(&plan, &test_scene(), &provider, None).expect("review");

    assert_eq!(result, Some(revised));
    assert_eq!(provider.calls(), 1);
}

#[test]
fn review_once_returns_none_when_the_provider_keeps_the_plan() {
    let provider = MockVisionProvider::new(VisionCritique {
        revised_plan: None,
        notes: "looks good".to_string(),
    });

    let result = review_once(&test_plan(), &test_scene(), &provider, None).expect("review");

    assert_eq!(result, None);
    assert_eq!(provider.calls(), 1);
}

#[test]
fn provider_errors_propagate() {
    let provider = MockVisionProvider::with_error("vision endpoint exploded");

    let error = review_once(&test_plan(), &test_scene(), &provider, None)
        .expect_err("provider failure must surface");

    assert!(
        matches!(&error, VisionError::Provider(message) if message.contains("exploded")),
        "unexpected error: {error}"
    );
    assert_eq!(provider.calls(), 1);
}

#[test]
fn cache_hit_avoids_a_second_provider_call() {
    let dir = tempfile::tempdir().expect("temp dir");
    let cache = ProjectCache::discover(dir.path());
    let plan = test_plan();
    let scene = test_scene();
    let revised = ScenePlan {
        title: "Cached revision".to_string(),
        ..plan.clone()
    };
    let critique = VisionCritique {
        revised_plan: Some(revised.clone()),
        notes: "cached".to_string(),
    };
    let provider = MockVisionProvider::new(critique.clone());

    let first = review_once(&plan, &scene, &provider, Some(&cache)).expect("first review");
    assert_eq!(first, Some(revised.clone()));
    assert_eq!(provider.calls(), 1);

    let second = review_once(&plan, &scene, &provider, Some(&cache)).expect("second review");
    assert_eq!(second, Some(revised));
    assert_eq!(
        provider.calls(),
        1,
        "a cache hit must not call the provider"
    );

    let key = critique_cache_key(&plan, None).expect("cache key");
    let stored = cache
        .get("vision", &key)
        .expect("read cache")
        .expect("cached critique");
    let decoded: VisionCritique = serde_json::from_slice(&stored).expect("critique json");
    assert_eq!(decoded, critique);
    assert_ne!(
        critique_cache_key(&plan, Some("gpt-4o")).expect("model key"),
        key,
        "the model must participate in the cache key"
    );
}

#[test]
fn cache_failures_do_not_block_the_provider() {
    let dir = tempfile::tempdir().expect("temp dir");
    std::fs::write(dir.path().join(".ascii-forge"), b"not a directory").expect("block cache");
    let cache = ProjectCache::discover(dir.path());
    let plan = test_plan();
    let revised = ScenePlan {
        title: "Offline revision".to_string(),
        ..plan.clone()
    };
    let provider = MockVisionProvider::new(VisionCritique {
        revised_plan: Some(revised.clone()),
        notes: "cache is broken".to_string(),
    });

    let result = review_once(&plan, &test_scene(), &provider, Some(&cache)).expect("review");

    assert_eq!(result, Some(revised));
    assert_eq!(provider.calls(), 1);
}

#[test]
fn openai_provider_rejects_empty_configuration() {
    let base = "https://api.openai.com/v1";
    let model = "gpt-4o";

    for result in [
        OpenAiVisionProvider::new("", base, model),
        OpenAiVisionProvider::new("   ", base, model),
        OpenAiVisionProvider::new("sk-test", "", model),
        OpenAiVisionProvider::new("sk-test", base, ""),
        OpenAiVisionProvider::new("sk-test", "api.openai.com/v1", model),
    ] {
        assert!(
            matches!(result, Err(VisionError::Config(_))),
            "empty or malformed configuration must be rejected"
        );
    }

    let provider = OpenAiVisionProvider::new("sk-secret", base, model).expect("valid config");
    assert!(
        !format!("{provider:?}").contains("sk-secret"),
        "Debug must not leak the API key"
    );
}

#[test]
fn request_body_carries_a_prompt_and_three_png_data_urls() {
    let request = VisionRequest {
        plan: test_plan(),
        frames: render_review_frames(&test_scene()).expect("review frames"),
    };

    let body = build_request_body("gpt-4o", &request).expect("request body");

    assert_eq!(body["model"].as_str(), Some("gpt-4o"));
    let content = body["messages"][0]["content"]
        .as_array()
        .expect("content parts");
    assert_eq!(
        content.iter().filter(|part| part["type"] == "text").count(),
        1
    );
    let prompt = content[0]["text"].as_str().expect("prompt text");
    assert!(prompt.contains("revised_plan"));
    assert!(
        prompt.contains("Hello"),
        "the current plan is part of the prompt"
    );

    let images: Vec<&serde_json::Value> = content
        .iter()
        .filter(|part| part["type"] == "image_url")
        .collect();
    assert_eq!(images.len(), 3, "exactly three frames are reviewed");
    for (index, image) in images.iter().enumerate() {
        let url = image["image_url"]["url"].as_str().expect("image url");
        let payload = url
            .strip_prefix("data:image/png;base64,")
            .unwrap_or_else(|| panic!("frame {index} must be a PNG data URL, got {url}"));
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(payload)
            .expect("base64 payload");
        assert!(
            bytes.starts_with(&[0x89, b'P', b'N', b'G']),
            "frame {index} must decode to a PNG"
        );
    }
}

#[test]
fn critique_parsing_is_defensive() {
    let fenced = parse_critique("```json\n{\"revised_plan\": null, \"notes\": \"ok\"}\n```")
        .expect("fenced critique");
    assert_eq!(
        fenced,
        VisionCritique {
            revised_plan: None,
            notes: "ok".to_string(),
        }
    );

    assert!(matches!(
        parse_critique("this is not JSON"),
        Err(VisionError::MalformedCritique(_))
    ));
}
