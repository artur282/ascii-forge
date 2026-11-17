//! Behavior tests for scene IR validation (spec S1).

use ascii_forge::scene::{
    Align, Canvas, Easing, Effect, EffectKind, FigletLayer, Keyframe, Layer, LayerKind,
    OutputFormat, OutputSpec, Rgba, SCENE_VERSION, Scene, TextLayer, Timeline, Transform,
    ValidationError,
};

fn valid_scene() -> Scene {
    Scene {
        version: SCENE_VERSION,
        seed: 7,
        canvas: Canvas {
            cols: 80,
            rows: 24,
            cell_aspect: 0.542,
            background: Rgba::rgb(10, 12, 16),
            foreground: Rgba::rgb(0, 255, 136),
        },
        layers: vec![
            Layer {
                id: "title".to_string(),
                kind: LayerKind::Text(TextLayer {
                    text: "Hello".to_string(),
                    color: None,
                    align: Align::Center,
                }),
                transform: Transform::default(),
                keyframes: vec![
                    Keyframe {
                        frame: 0,
                        transform: Transform::default(),
                        easing: Easing::Linear,
                    },
                    Keyframe {
                        frame: 10,
                        transform: Transform {
                            x: 4,
                            y: 2,
                            scale: 2.0,
                            opacity: 1.0,
                        },
                        easing: Easing::EaseInOut,
                    },
                ],
            },
            Layer {
                id: "banner".to_string(),
                kind: LayerKind::Figlet(FigletLayer {
                    text: "ASCII".to_string(),
                    font: "standard".to_string(),
                    color: None,
                }),
                transform: Transform::default(),
                keyframes: Vec::new(),
            },
        ],
        timeline: Timeline {
            fps: 30,
            frames: 60,
            loop_: true,
        },
        effects: vec![
            Effect {
                id: "type".to_string(),
                kind: EffectKind::Typewriter {
                    chars_per_frame: 1.0,
                },
                target: Some("title".to_string()),
                start: 0,
                end: 30,
            },
            Effect {
                id: "fade".to_string(),
                kind: EffectKind::Fade { from: 0.0, to: 1.0 },
                target: None,
                start: 30,
                end: 60,
            },
        ],
        output: OutputSpec {
            width: 1920,
            height: 1080,
            format: OutputFormat::Png,
        },
    }
}

fn scene_without_timeline_constraints() -> Scene {
    let mut scene = valid_scene();
    for layer in &mut scene.layers {
        layer.keyframes.clear();
    }
    scene.effects.clear();
    scene
}

fn errors_of(scene: &Scene) -> Vec<ValidationError> {
    scene.validate().expect_err("scene should be invalid")
}

fn assert_single_error(scene: &Scene, expected: impl Fn(&ValidationError) -> bool) {
    let errors = errors_of(scene);
    assert_eq!(
        errors.len(),
        1,
        "expected exactly one error, got {errors:?}"
    );
    assert!(expected(&errors[0]), "unexpected error: {}", errors[0]);
}

#[test]
fn valid_scene_passes_validation() {
    assert_eq!(valid_scene().validate(), Ok(()));
}

#[test]
fn scene_schema_does_not_describe_image_layers() {
    let schema = schemars::schema_for!(Scene);
    let rendered = serde_json::to_string(&schema).expect("schema must serialize to JSON");

    assert!(
        !rendered.contains("ImageLayer"),
        "schema must not define ImageLayer: {rendered}"
    );
    assert!(
        !rendered.contains("\"image\""),
        "schema must not describe an image layer kind: {rendered}"
    );
}

#[test]
fn image_layers_are_rejected_by_deserialization() {
    let json = serde_json::json!({
        "version": SCENE_VERSION,
        "seed": 0,
        "canvas": { "cols": 8, "rows": 2 },
        "layers": [{
            "id": "legacy",
            "kind": { "type": "image", "source": "photo.png" }
        }],
        "timeline": { "fps": 30, "frames": 1 }
    });

    let error = serde_json::from_value::<Scene>(json).expect_err("image layers are not supported");
    assert!(
        error.to_string().contains("unknown variant `image`"),
        "unexpected error: {error}"
    );
}

#[test]
fn collects_all_errors_instead_of_stopping_at_the_first() {
    let mut scene = valid_scene();
    scene.version = SCENE_VERSION + 1;
    scene.canvas.cols = 0;
    scene.timeline.fps = 0;
    scene.output.width = 0;

    let errors = errors_of(&scene);
    assert_eq!(errors.len(), 4, "{errors:?}");
    assert!(
        errors
            .iter()
            .any(|error| matches!(error, ValidationError::UnsupportedVersion { .. }))
    );
    assert!(
        errors
            .iter()
            .any(|error| matches!(error, ValidationError::CanvasColsOutOfRange { .. }))
    );
    assert!(
        errors
            .iter()
            .any(|error| matches!(error, ValidationError::FpsOutOfRange { .. }))
    );
    assert!(
        errors
            .iter()
            .any(|error| matches!(error, ValidationError::OutputWidthOutOfRange { .. }))
    );
}

#[test]
fn rejects_unsupported_scene_version() {
    let mut scene = valid_scene();
    scene.version = SCENE_VERSION + 1;

    assert_single_error(&scene, |error| {
        matches!(
            error,
            ValidationError::UnsupportedVersion { found, expected }
                if *found == SCENE_VERSION + 1 && *expected == SCENE_VERSION
        )
    });
}

#[test]
fn rejects_canvas_dimensions_out_of_range() {
    let mut scene = valid_scene();
    scene.canvas.cols = 0;
    scene.canvas.rows = 513;

    let errors = errors_of(&scene);
    assert_eq!(errors.len(), 2, "{errors:?}");
    assert!(errors.iter().any(|error| matches!(
        error,
        ValidationError::CanvasColsOutOfRange { value: 0, .. }
    )));
    assert!(errors.iter().any(|error| matches!(
        error,
        ValidationError::CanvasRowsOutOfRange { value: 513, .. }
    )));
}

#[test]
fn accepts_canvas_dimensions_at_bounds() {
    let mut scene = valid_scene();
    scene.canvas.cols = 1;
    scene.canvas.rows = 512;
    assert_eq!(scene.validate(), Ok(()));

    scene.canvas.cols = 512;
    scene.canvas.rows = 1;
    assert_eq!(scene.validate(), Ok(()));
}

#[test]
fn rejects_cell_aspect_out_of_range() {
    for aspect in [0.1, 2.1] {
        let mut scene = valid_scene();
        scene.canvas.cell_aspect = aspect;
        assert_single_error(&scene, |error| {
            matches!(error, ValidationError::CellAspectOutOfRange { .. })
        });
    }
}

#[test]
fn accepts_cell_aspect_at_bounds() {
    for aspect in [0.2, 2.0] {
        let mut scene = valid_scene();
        scene.canvas.cell_aspect = aspect;
        assert_eq!(scene.validate(), Ok(()));
    }
}

#[test]
fn rejects_fps_and_frames_out_of_range() {
    let mut scene = scene_without_timeline_constraints();
    scene.timeline.fps = 0;
    scene.timeline.frames = 0;
    let errors = errors_of(&scene);
    assert_eq!(errors.len(), 2, "{errors:?}");
    assert!(
        errors
            .iter()
            .any(|error| matches!(error, ValidationError::FpsOutOfRange { value: 0, .. }))
    );
    assert!(
        errors
            .iter()
            .any(|error| matches!(error, ValidationError::FramesOutOfRange { value: 0, .. }))
    );

    scene.timeline.fps = 61;
    scene.timeline.frames = 18_001;
    let errors = errors_of(&scene);
    assert_eq!(errors.len(), 2, "{errors:?}");
    assert!(
        errors
            .iter()
            .any(|error| matches!(error, ValidationError::FpsOutOfRange { value: 61, .. }))
    );
    assert!(errors.iter().any(|error| matches!(
        error,
        ValidationError::FramesOutOfRange { value: 18_001, .. }
    )));
}

#[test]
fn accepts_timeline_bounds() {
    let mut scene = scene_without_timeline_constraints();
    scene.timeline.fps = 1;
    scene.timeline.frames = 1;
    assert_eq!(scene.validate(), Ok(()));

    scene.timeline.fps = 60;
    scene.timeline.frames = 18_000;
    assert_eq!(scene.validate(), Ok(()));
}

#[test]
fn rejects_duplicate_layer_ids() {
    let mut scene = valid_scene();
    let duplicate = scene.layers[0].clone();
    scene.layers.push(duplicate);

    assert_single_error(
        &scene,
        |error| matches!(error, ValidationError::DuplicateLayerId { id } if id.as_str() == "title"),
    );
}

#[test]
fn rejects_effect_target_that_does_not_exist() {
    let mut scene = valid_scene();
    scene.effects[0].target = Some("missing".to_string());

    assert_single_error(&scene, |error| {
        matches!(
            error,
            ValidationError::EffectTargetNotFound { target, .. } if target.as_str() == "missing"
        )
    });
}

#[test]
fn rejects_out_of_order_keyframes() {
    let mut scene = valid_scene();
    scene.layers[0].keyframes = vec![
        Keyframe {
            frame: 10,
            transform: Transform::default(),
            easing: Easing::Linear,
        },
        Keyframe {
            frame: 5,
            transform: Transform::default(),
            easing: Easing::Linear,
        },
    ];

    assert_single_error(&scene, |error| {
        matches!(
            error,
            ValidationError::KeyframesOutOfOrder {
                frame: 5,
                previous: 10,
                ..
            }
        )
    });
}

#[test]
fn rejects_keyframe_at_or_past_timeline_end() {
    let mut scene = valid_scene();
    scene.layers[0].keyframes[1].frame = 60;

    assert_single_error(&scene, |error| {
        matches!(
            error,
            ValidationError::KeyframeOutOfRange {
                frame: 60,
                frames: 60,
                ..
            }
        )
    });
}

#[test]
fn rejects_opacity_out_of_range() {
    let mut scene = valid_scene();
    scene.layers[0].transform.opacity = 1.5;

    assert_single_error(&scene, |error| {
        matches!(
            error,
            ValidationError::TransformOpacityOutOfRange { value, .. } if *value == 1.5
        )
    });
}

#[test]
fn rejects_scale_and_offsets_out_of_range() {
    let mut scene = valid_scene();
    scene.layers[0].transform = Transform {
        x: 10_001,
        y: -10_001,
        scale: 0.05,
        opacity: 1.0,
    };

    let errors = errors_of(&scene);
    assert_eq!(errors.len(), 3, "{errors:?}");
    assert!(
        errors
            .iter()
            .any(|error| matches!(error, ValidationError::TransformScaleOutOfRange { .. }))
    );
    assert!(errors.iter().any(|error| matches!(
        error,
        ValidationError::TransformXOutOfRange { value: 10_001, .. }
    )));
    assert!(errors.iter().any(|error| matches!(
        error,
        ValidationError::TransformYOutOfRange { value: -10_001, .. }
    )));
}

#[test]
fn accepts_transform_bounds() {
    let mut scene = valid_scene();
    scene.layers[0].transform = Transform {
        x: 10_000,
        y: -10_000,
        scale: 0.1,
        opacity: 0.0,
    };
    assert_eq!(scene.validate(), Ok(()));

    scene.layers[0].transform = Transform {
        x: -10_000,
        y: 10_000,
        scale: 10.0,
        opacity: 1.0,
    };
    assert_eq!(scene.validate(), Ok(()));
}

#[test]
fn rejects_invalid_keyframe_transform() {
    let mut scene = valid_scene();
    scene.layers[0].keyframes[1].transform.scale = 11.0;

    assert_single_error(&scene, |error| {
        matches!(
            error,
            ValidationError::TransformScaleOutOfRange { location, .. }
                if location.contains("keyframe")
        )
    });
}

#[test]
fn rejects_invalid_cubic_bezier_control_points() {
    let mut scene = valid_scene();
    scene.layers[0].keyframes[0].easing = Easing::CubicBezier {
        x1: 1.5,
        y1: 0.0,
        x2: 0.5,
        y2: 1.0,
    };

    assert_single_error(&scene, |error| {
        matches!(
            error,
            ValidationError::InvalidCubicBezier { layer, frame: 0, .. } if layer == "title"
        )
    });
}

#[test]
fn accepts_cubic_bezier_bounds() {
    let mut scene = valid_scene();
    scene.layers[0].keyframes[0].easing = Easing::CubicBezier {
        x1: 0.0,
        y1: 0.0,
        x2: 1.0,
        y2: 1.0,
    };
    assert_eq!(scene.validate(), Ok(()));
}

#[test]
fn rejects_empty_and_oversized_layer_text() {
    let mut scene = valid_scene();
    scene.layers[0].kind = LayerKind::Text(TextLayer {
        text: String::new(),
        color: None,
        align: Align::Center,
    });
    scene.layers[1].kind = LayerKind::Figlet(FigletLayer {
        text: "x".repeat(201),
        font: "standard".to_string(),
        color: None,
    });

    let errors = errors_of(&scene);
    assert_eq!(errors.len(), 2, "{errors:?}");
    assert!(errors.iter().any(
        |error| matches!(error, ValidationError::EmptyLayerText { layer } if layer.as_str() == "title")
    ));
    assert!(errors.iter().any(|error| {
        matches!(
            error,
            ValidationError::LayerTextTooLong {
                layer,
                len: 201,
                ..
            } if layer.as_str() == "banner"
        )
    }));
}

#[test]
fn rejects_invalid_figlet_font() {
    let mut scene = valid_scene();
    scene.layers[1].kind = LayerKind::Figlet(FigletLayer {
        text: "ASCII".to_string(),
        font: String::new(),
        color: None,
    });
    assert_single_error(
        &scene,
        |error| matches!(error, ValidationError::EmptyFigletFont { layer } if layer.as_str() == "banner"),
    );

    scene.layers[1].kind = LayerKind::Figlet(FigletLayer {
        text: "ASCII".to_string(),
        font: "f".repeat(65),
        color: None,
    });
    assert_single_error(&scene, |error| {
        matches!(error, ValidationError::FigletFontTooLong { len: 65, .. })
    });
}

#[test]
fn rejects_duplicate_effect_ids() {
    let mut scene = valid_scene();
    let mut duplicate = scene.effects[0].clone();
    duplicate.start = 0;
    duplicate.end = 10;
    scene.effects.push(duplicate);

    assert_single_error(
        &scene,
        |error| matches!(error, ValidationError::DuplicateEffectId { id } if id.as_str() == "type"),
    );
}

#[test]
fn rejects_invalid_effect_ranges() {
    let mut scene = valid_scene();
    scene.effects[0].start = 30;
    scene.effects[0].end = 30;
    scene.effects[1].end = 61;

    let errors = errors_of(&scene);
    assert_eq!(errors.len(), 2, "{errors:?}");
    assert!(errors.iter().any(|error| {
        matches!(
            error,
            ValidationError::EffectRangeInvalid {
                id,
                start: 30,
                end: 30,
            } if id.as_str() == "type"
        )
    }));
    assert!(errors.iter().any(|error| {
        matches!(
            error,
            ValidationError::EffectEndOutOfRange {
                id,
                end: 61,
                frames: 60,
            } if id.as_str() == "fade"
        )
    }));
}

#[test]
fn rejects_float_effect_parameters_out_of_range() {
    let mut scene = valid_scene();
    scene.effects[0].kind = EffectKind::Typewriter {
        chars_per_frame: 0.0,
    };
    scene.effects[1].kind = EffectKind::Fade { from: 0.0, to: 1.5 };

    let errors = errors_of(&scene);
    assert_eq!(errors.len(), 2, "{errors:?}");
    assert!(errors.iter().any(|error| {
        matches!(
            error,
            ValidationError::EffectParameterOutOfRange {
                parameter: "chars_per_frame",
                ..
            }
        )
    }));
    assert!(errors.iter().any(|error| {
        matches!(
            error,
            ValidationError::EffectParameterOutOfRange {
                parameter: "to",
                ..
            }
        )
    }));
}

#[test]
fn rejects_integer_effect_parameters_out_of_range() {
    let mut scene = valid_scene();
    scene.effects = vec![
        Effect {
            id: "slide".to_string(),
            kind: EffectKind::Slide { dx: 10_001, dy: 0 },
            target: None,
            start: 0,
            end: 10,
        },
        Effect {
            id: "blink".to_string(),
            kind: EffectKind::Blink { period: 0 },
            target: None,
            start: 10,
            end: 20,
        },
    ];

    let errors = errors_of(&scene);
    assert_eq!(errors.len(), 2, "{errors:?}");
    assert!(errors.iter().any(|error| {
        matches!(
            error,
            ValidationError::EffectOffsetOutOfRange {
                parameter: "dx",
                value: 10_001,
                ..
            }
        )
    }));
    assert!(errors.iter().any(|error| {
        matches!(
            error,
            ValidationError::EffectPeriodOutOfRange { value: 0, .. }
        )
    }));
}

#[test]
fn rejects_output_dimensions_out_of_range() {
    let mut scene = valid_scene();
    scene.output.width = 15;
    scene.output.height = 8193;

    let errors = errors_of(&scene);
    assert_eq!(errors.len(), 2, "{errors:?}");
    assert!(errors.iter().any(|error| matches!(
        error,
        ValidationError::OutputWidthOutOfRange { value: 15, .. }
    )));
    assert!(errors.iter().any(|error| matches!(
        error,
        ValidationError::OutputHeightOutOfRange { value: 8193, .. }
    )));
}

#[test]
fn accepts_output_dimensions_at_bounds() {
    let mut scene = valid_scene();
    scene.output.width = 16;
    scene.output.height = 8192;
    assert_eq!(scene.validate(), Ok(()));
}

#[test]
fn rejects_odd_dimensions_for_video_formats() {
    for format in [OutputFormat::Mp4, OutputFormat::WebM] {
        let mut scene = valid_scene();
        scene.output = OutputSpec {
            width: 1920,
            height: 1081,
            format,
        };
        assert_single_error(&scene, |error| {
            matches!(
                error,
                ValidationError::OddVideoDimensions {
                    width: 1920,
                    height: 1081,
                    ..
                }
            )
        });
    }
}

#[test]
fn accepts_odd_dimensions_for_still_formats() {
    let mut scene = valid_scene();
    scene.output = OutputSpec {
        width: 1921,
        height: 1081,
        format: OutputFormat::Png,
    };
    assert_eq!(scene.validate(), Ok(()));
}
