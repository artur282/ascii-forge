use ascii_forge::ai::compile::CompileError;
use ascii_forge::ai::plan::PlanValidationError;
use ascii_forge::ai::{
    Aspect, Beat, BeatTarget, Layout, PlanEffect, ScenePlan, TemplateKind, compile_plan,
};
use ascii_forge::scene::{
    Align, EffectKind, Layer, LayerKind, OutputFormat, Rgba, SCENE_VERSION, Scene,
};

fn palette() -> Vec<Rgba> {
    vec![
        Rgba::rgb(10, 12, 16),
        Rgba::rgb(0, 255, 136),
        Rgba::rgb(255, 255, 255),
    ]
}

fn title_beat() -> Beat {
    Beat {
        effect: PlanEffect::Typewriter {
            chars_per_frame: 1.0,
        },
        target: BeatTarget::Title,
        frames: 30,
    }
}

fn valid_plan() -> ScenePlan {
    ScenePlan {
        title: "ASCII Forge".to_string(),
        tagline: Some("Scenes from plain text".to_string()),
        template: TemplateKind::Logo,
        aspect: Aspect::Landscape,
        palette: palette(),
        font: "standard".to_string(),
        layout: Layout::Centered,
        beats: vec![title_beat()],
        seed: 42,
        align: Align::Center,
    }
}

fn plan_with_effect(effect: PlanEffect) -> ScenePlan {
    let mut plan = valid_plan();
    plan.beats = vec![Beat {
        effect,
        target: BeatTarget::Title,
        frames: 10,
    }];
    plan
}

fn validation_errors(plan: &ScenePlan) -> Vec<PlanValidationError> {
    plan.validate().expect_err("plan should be invalid")
}

fn layer<'a>(scene: &'a Scene, id: &str) -> &'a Layer {
    scene
        .layers
        .iter()
        .find(|layer| layer.id == id)
        .unwrap_or_else(|| panic!("missing layer \"{id}\""))
}

fn expected_offsets(layout: Layout) -> (i32, i32) {
    match layout {
        Layout::Centered => (4, 12),
        Layout::Stacked => (2, 10),
        Layout::LowerThird => (15, 21),
        Layout::FullBleed => (6, 16),
    }
}

fn has_effect_parameter_error(errors: &[PlanValidationError], parameter: &str) -> bool {
    errors.iter().any(|error| {
        matches!(
            error,
            PlanValidationError::EffectParameterOutOfRange {
                parameter: found,
                ..
            } if *found == parameter
        )
    })
}

fn has_not_finite_error(errors: &[PlanValidationError], parameter: &str) -> bool {
    errors.iter().any(|error| {
        matches!(
            error,
            PlanValidationError::EffectParameterNotFinite {
                parameter: found,
                ..
            } if *found == parameter
        )
    })
}

#[test]
fn valid_plan_compiles_to_a_scene_that_passes_scene_validation() {
    let plan = valid_plan();
    plan.validate().expect("the fixture plan must be valid");

    let scene = compile_plan(&plan).expect("the fixture plan must compile");
    scene.validate().expect("the compiled scene must be valid");

    assert_eq!(scene.version, SCENE_VERSION);
    assert_eq!(scene.seed, plan.seed);
    assert_eq!(scene.canvas.cols, 80);
    assert_eq!(scene.canvas.rows, 24);
    assert_eq!(scene.canvas.cell_aspect, 0.542);
    assert_eq!(scene.canvas.background, plan.palette[0]);
    assert_eq!(scene.canvas.foreground, plan.palette[1]);

    assert_eq!(scene.timeline.fps, 30);
    assert_eq!(scene.timeline.frames, 30);
    assert!(scene.timeline.loop_);
    assert!(matches!(scene.output.format, OutputFormat::Gif));
    assert_eq!(
        (scene.output.width, scene.output.height),
        Aspect::Landscape.dimensions()
    );

    let title = layer(&scene, "title");
    match &title.kind {
        LayerKind::Figlet(figlet) => {
            assert_eq!(figlet.text, plan.title);
            assert_eq!(figlet.font, plan.font);
            assert_eq!(figlet.color, Some(plan.palette[1]));
        }
        other => panic!("title must be a FIGlet layer, got {other:?}"),
    }
    assert_eq!(title.transform.x, 0);
    assert_eq!(title.transform.y, 4);

    let tagline = layer(&scene, "tagline");
    match &tagline.kind {
        LayerKind::Text(text) => {
            assert_eq!(text.text, plan.tagline.clone().unwrap());
            assert_eq!(text.color, Some(plan.palette[2]));
            assert_eq!(text.align, plan.align);
        }
        other => panic!("tagline must be a text layer, got {other:?}"),
    }
    assert_eq!(tagline.transform.x, 0);
    assert_eq!(tagline.transform.y, 12);

    assert_eq!(scene.effects.len(), 1);
    assert_eq!(scene.effects[0].id, "beat-0");
    assert_eq!(scene.effects[0].target.as_deref(), Some("title"));
    assert_eq!((scene.effects[0].start, scene.effects[0].end), (0, 30));
    assert!(matches!(
        scene.effects[0].kind,
        EffectKind::Typewriter { .. }
    ));
}

#[test]
fn plan_without_tagline_compiles_to_only_the_title_layer() {
    let mut plan = valid_plan();
    plan.tagline = None;

    let scene = compile_plan(&plan).expect("plan without tagline must compile");
    scene.validate().expect("scene must be valid");

    assert_eq!(scene.layers.len(), 1);
    assert_eq!(scene.layers[0].id, "title");
}

#[test]
fn tagline_accent_falls_back_to_the_foreground_color() {
    let mut plan = valid_plan();
    plan.palette = vec![Rgba::rgb(0, 0, 0), Rgba::rgb(0, 255, 136)];

    let scene = compile_plan(&plan).expect("plan with two colors must compile");
    let tagline = layer(&scene, "tagline");
    match &tagline.kind {
        LayerKind::Text(text) => assert_eq!(text.color, Some(plan.palette[1])),
        other => panic!("tagline must be a text layer, got {other:?}"),
    }
}

#[test]
fn title_must_not_be_blank() {
    let mut plan = valid_plan();
    plan.title = "   ".to_string();

    assert!(validation_errors(&plan).contains(&PlanValidationError::EmptyTitle));
}

#[test]
fn title_length_is_bounded() {
    let mut plan = valid_plan();
    plan.title = "x".repeat(81);

    assert!(
        validation_errors(&plan).contains(&PlanValidationError::TitleTooLong { len: 81, max: 80 })
    );

    plan.title = "x".repeat(80);
    plan.validate().expect("an 80 character title is valid");
}

#[test]
fn tagline_length_is_bounded_and_blank_taglines_are_rejected() {
    let mut plan = valid_plan();

    plan.tagline = Some("   ".to_string());
    assert!(validation_errors(&plan).contains(&PlanValidationError::BlankTagline));

    plan.tagline = Some("x".repeat(121));
    assert!(
        validation_errors(&plan)
            .contains(&PlanValidationError::TaglineTooLong { len: 121, max: 120 })
    );

    plan.tagline = Some("x".repeat(120));
    plan.validate().expect("a 120 character tagline is valid");

    plan.tagline = None;
    plan.validate().expect("the tagline is optional");
}

#[test]
fn palette_size_is_bounded() {
    let mut plan = valid_plan();

    plan.palette = palette()[..1].to_vec();
    assert!(
        validation_errors(&plan).contains(&PlanValidationError::PaletteSizeOutOfRange {
            len: 1,
            min: 2,
            max: 6,
        })
    );

    plan.palette = vec![Rgba::rgb(0, 0, 0); 7];
    assert!(
        validation_errors(&plan).contains(&PlanValidationError::PaletteSizeOutOfRange {
            len: 7,
            min: 2,
            max: 6,
        })
    );

    plan.palette = vec![Rgba::rgb(0, 0, 0); 2];
    plan.validate().expect("a two color palette is valid");

    plan.palette = vec![Rgba::rgb(0, 0, 0); 6];
    plan.validate().expect("a six color palette is valid");
}

#[test]
fn font_must_be_a_bundled_figlet_font() {
    let mut plan = valid_plan();
    plan.font = "comic".to_string();

    let errors = validation_errors(&plan);
    assert!(errors.iter().any(|error| matches!(
        error,
        PlanValidationError::UnknownFont { font, supported }
            if font == "comic" && supported.contains("standard")
    )));
}

#[test]
fn beat_count_is_bounded() {
    let mut plan = valid_plan();

    plan.beats = Vec::new();
    assert!(
        validation_errors(&plan).contains(&PlanValidationError::BeatCountOutOfRange {
            len: 0,
            min: 1,
            max: 8,
        })
    );

    plan.beats = (0..9)
        .map(|_| Beat {
            effect: PlanEffect::None,
            target: BeatTarget::Title,
            frames: 1,
        })
        .collect();
    assert!(
        validation_errors(&plan).contains(&PlanValidationError::BeatCountOutOfRange {
            len: 9,
            min: 1,
            max: 8,
        })
    );

    plan.beats = (0..8)
        .map(|_| Beat {
            effect: PlanEffect::None,
            target: BeatTarget::Title,
            frames: 1,
        })
        .collect();
    plan.validate().expect("eight beats are valid");
}

#[test]
fn beat_frames_are_bounded() {
    let mut plan = valid_plan();

    plan.beats[0].frames = 0;
    assert!(
        validation_errors(&plan).contains(&PlanValidationError::BeatFramesOutOfRange {
            beat: 0,
            frames: 0,
            min: 1,
            max: 300,
        })
    );

    plan.beats[0].frames = 301;
    assert!(
        validation_errors(&plan).contains(&PlanValidationError::BeatFramesOutOfRange {
            beat: 0,
            frames: 301,
            min: 1,
            max: 300,
        })
    );

    plan.beats[0].frames = 1;
    plan.validate().expect("one frame is valid");

    plan.beats[0].frames = 300;
    plan.validate().expect("300 frames is valid");
}

#[test]
fn total_frames_are_bounded() {
    let mut plan = valid_plan();
    plan.beats = (0..8)
        .map(|_| Beat {
            effect: PlanEffect::None,
            target: BeatTarget::Title,
            frames: 2_500,
        })
        .collect();

    let errors = validation_errors(&plan);
    assert!(errors.iter().any(|error| matches!(
        error,
        PlanValidationError::TotalFramesExceeded { frames, max }
            if *frames == 20_000 && *max == 18_000
    )));
}

#[test]
fn tagline_target_requires_a_tagline() {
    let mut plan = valid_plan();
    plan.tagline = None;
    plan.beats = vec![Beat {
        effect: PlanEffect::Fade { from: 0.0, to: 1.0 },
        target: BeatTarget::Tagline,
        frames: 10,
    }];

    assert!(
        validation_errors(&plan)
            .contains(&PlanValidationError::TaglineTargetWithoutTagline { beat: 0 })
    );

    plan.tagline = Some("Scenes from plain text".to_string());
    plan.validate()
        .expect("a tagline target is valid once a tagline exists");
}

#[test]
fn typewriter_chars_per_frame_is_bounded() {
    let too_slow = plan_with_effect(PlanEffect::Typewriter {
        chars_per_frame: 0.0,
    });
    assert!(has_effect_parameter_error(
        &validation_errors(&too_slow),
        "chars_per_frame"
    ));

    let too_fast = plan_with_effect(PlanEffect::Typewriter {
        chars_per_frame: 100.5,
    });
    assert!(has_effect_parameter_error(
        &validation_errors(&too_fast),
        "chars_per_frame"
    ));

    let not_finite = plan_with_effect(PlanEffect::Typewriter {
        chars_per_frame: f32::NAN,
    });
    assert!(has_not_finite_error(
        &validation_errors(&not_finite),
        "chars_per_frame"
    ));

    plan_with_effect(PlanEffect::Typewriter {
        chars_per_frame: 0.01,
    })
    .validate()
    .expect("0.01 chars per frame is valid");
    plan_with_effect(PlanEffect::Typewriter {
        chars_per_frame: 100.0,
    })
    .validate()
    .expect("100 chars per frame is valid");
}

#[test]
fn fade_opacity_is_bounded() {
    let negative_from = plan_with_effect(PlanEffect::Fade {
        from: -0.1,
        to: 1.0,
    });
    assert!(has_effect_parameter_error(
        &validation_errors(&negative_from),
        "from"
    ));

    let too_opaque = plan_with_effect(PlanEffect::Fade { from: 0.0, to: 1.1 });
    assert!(has_effect_parameter_error(
        &validation_errors(&too_opaque),
        "to"
    ));

    let not_finite = plan_with_effect(PlanEffect::Fade {
        from: f32::INFINITY,
        to: 1.0,
    });
    assert!(has_not_finite_error(
        &validation_errors(&not_finite),
        "from"
    ));

    plan_with_effect(PlanEffect::Fade { from: 0.0, to: 1.0 })
        .validate()
        .expect("fade bounds are valid");
}

#[test]
fn glitch_and_scanline_parameters_are_bounded() {
    let glitch = plan_with_effect(PlanEffect::Glitch { intensity: 1.1 });
    assert!(has_effect_parameter_error(
        &validation_errors(&glitch),
        "intensity"
    ));

    let scanline_intensity = plan_with_effect(PlanEffect::Scanline {
        intensity: -0.5,
        speed: 1.0,
    });
    assert!(has_effect_parameter_error(
        &validation_errors(&scanline_intensity),
        "intensity"
    ));

    let scanline_speed = plan_with_effect(PlanEffect::Scanline {
        intensity: 0.5,
        speed: 20.5,
    });
    assert!(has_effect_parameter_error(
        &validation_errors(&scanline_speed),
        "speed"
    ));

    let not_finite = plan_with_effect(PlanEffect::Scanline {
        intensity: 0.5,
        speed: f32::NAN,
    });
    assert!(has_not_finite_error(
        &validation_errors(&not_finite),
        "speed"
    ));

    plan_with_effect(PlanEffect::Glitch { intensity: 1.0 })
        .validate()
        .expect("intensity 1 is valid");
    plan_with_effect(PlanEffect::Scanline {
        intensity: 1.0,
        speed: 20.0,
    })
    .validate()
    .expect("scanline bounds are valid");
}

#[test]
fn wave_parameters_are_bounded() {
    let amplitude = plan_with_effect(PlanEffect::Wave {
        amplitude: 100.5,
        frequency: 0.1,
    });
    assert!(has_effect_parameter_error(
        &validation_errors(&amplitude),
        "amplitude"
    ));

    let frequency = plan_with_effect(PlanEffect::Wave {
        amplitude: 1.0,
        frequency: 50.5,
    });
    assert!(has_effect_parameter_error(
        &validation_errors(&frequency),
        "frequency"
    ));

    let not_finite = plan_with_effect(PlanEffect::Wave {
        amplitude: f32::NAN,
        frequency: 0.1,
    });
    assert!(has_not_finite_error(
        &validation_errors(&not_finite),
        "amplitude"
    ));

    plan_with_effect(PlanEffect::Wave {
        amplitude: 100.0,
        frequency: 50.0,
    })
    .validate()
    .expect("wave bounds are valid");
}

#[test]
fn blink_period_is_bounded() {
    let zero = plan_with_effect(PlanEffect::Blink { period: 0 });
    assert!(
        validation_errors(&zero).contains(&PlanValidationError::EffectPeriodOutOfRange {
            beat: 0,
            value: 0,
            min: 1,
            max: 18_000,
        })
    );

    let too_long = plan_with_effect(PlanEffect::Blink { period: 18_001 });
    assert!(
        validation_errors(&too_long).contains(&PlanValidationError::EffectPeriodOutOfRange {
            beat: 0,
            value: 18_001,
            min: 1,
            max: 18_000,
        })
    );

    plan_with_effect(PlanEffect::Blink { period: 1 })
        .validate()
        .expect("period 1 is valid");
    plan_with_effect(PlanEffect::Blink { period: 18_000 })
        .validate()
        .expect("period 18000 is valid");
}

#[test]
fn slide_offsets_are_bounded() {
    let far_right = plan_with_effect(PlanEffect::Slide { dx: 20_000, dy: 0 });
    assert!(
        validation_errors(&far_right).contains(&PlanValidationError::EffectOffsetOutOfRange {
            beat: 0,
            parameter: "dx",
            value: 20_000,
            min: -10_000,
            max: 10_000,
        })
    );

    let far_up = plan_with_effect(PlanEffect::Slide { dx: 0, dy: -20_000 });
    assert!(
        validation_errors(&far_up).contains(&PlanValidationError::EffectOffsetOutOfRange {
            beat: 0,
            parameter: "dy",
            value: -20_000,
            min: -10_000,
            max: 10_000,
        })
    );

    plan_with_effect(PlanEffect::Slide {
        dx: 10_000,
        dy: -10_000,
    })
    .validate()
    .expect("slide bounds are valid");
}

#[test]
fn every_layout_and_aspect_compiles_with_expected_offsets_and_dimensions() {
    let aspects = [Aspect::Landscape, Aspect::Square, Aspect::Portrait];
    let layouts = [
        Layout::Centered,
        Layout::Stacked,
        Layout::LowerThird,
        Layout::FullBleed,
    ];

    for aspect in aspects {
        for layout in layouts {
            let mut plan = valid_plan();
            plan.aspect = aspect;
            plan.layout = layout;

            let scene = compile_plan(&plan)
                .unwrap_or_else(|error| panic!("{aspect:?}/{layout:?}: {error}"));
            scene
                .validate()
                .unwrap_or_else(|errors| panic!("{aspect:?}/{layout:?}: {errors:?}"));

            assert_eq!(
                (scene.output.width, scene.output.height),
                aspect.dimensions()
            );
            let (title_y, tagline_y) = expected_offsets(layout);
            assert_eq!(layer(&scene, "title").transform.y, title_y);
            assert_eq!(layer(&scene, "tagline").transform.y, tagline_y);
            assert_eq!(layer(&scene, "title").transform.x, 0);
            assert_eq!(layer(&scene, "tagline").transform.x, 0);
        }
    }
}

#[test]
fn every_plan_effect_maps_to_its_scene_effect() {
    let mut plan = valid_plan();
    plan.beats = vec![
        Beat {
            effect: PlanEffect::Typewriter {
                chars_per_frame: 2.0,
            },
            target: BeatTarget::Title,
            frames: 5,
        },
        Beat {
            effect: PlanEffect::Fade {
                from: 0.25,
                to: 0.75,
            },
            target: BeatTarget::Title,
            frames: 5,
        },
        Beat {
            effect: PlanEffect::Slide { dx: 3, dy: -4 },
            target: BeatTarget::Tagline,
            frames: 5,
        },
        Beat {
            effect: PlanEffect::Glitch { intensity: 0.6 },
            target: BeatTarget::Canvas,
            frames: 5,
        },
        Beat {
            effect: PlanEffect::Scanline {
                intensity: 0.4,
                speed: 2.0,
            },
            target: BeatTarget::Canvas,
            frames: 5,
        },
        Beat {
            effect: PlanEffect::Wave {
                amplitude: 2.0,
                frequency: 0.3,
            },
            target: BeatTarget::Title,
            frames: 5,
        },
        Beat {
            effect: PlanEffect::Dissolve,
            target: BeatTarget::Canvas,
            frames: 5,
        },
        Beat {
            effect: PlanEffect::Blink { period: 12 },
            target: BeatTarget::Tagline,
            frames: 5,
        },
    ];

    let scene = compile_plan(&plan).expect("all effects must compile");
    scene.validate().expect("scene must be valid");
    assert_eq!(scene.timeline.frames, 40);
    assert_eq!(scene.effects.len(), 8);

    for (index, effect) in scene.effects.iter().enumerate() {
        assert_eq!(effect.id, format!("beat-{index}"));
        assert_eq!(
            (effect.start, effect.end),
            (index as u32 * 5, index as u32 * 5 + 5)
        );
    }

    assert_eq!(scene.effects[0].target.as_deref(), Some("title"));
    assert!(
        matches!(scene.effects[0].kind, EffectKind::Typewriter { chars_per_frame }
            if (chars_per_frame - 2.0).abs() < f32::EPSILON)
    );

    assert_eq!(scene.effects[1].target.as_deref(), Some("title"));
    assert!(
        matches!(scene.effects[1].kind, EffectKind::Fade { from, to }
            if (from - 0.25).abs() < f32::EPSILON && (to - 0.75).abs() < f32::EPSILON)
    );

    assert_eq!(scene.effects[2].target.as_deref(), Some("tagline"));
    assert!(matches!(
        scene.effects[2].kind,
        EffectKind::Slide { dx: 3, dy: -4 }
    ));

    assert_eq!(scene.effects[3].target, None);
    assert!(
        matches!(scene.effects[3].kind, EffectKind::Glitch { intensity, seed }
            if (intensity - 0.6).abs() < f32::EPSILON && seed == plan.seed)
    );

    assert_eq!(scene.effects[4].target, None);
    assert!(
        matches!(scene.effects[4].kind, EffectKind::Scanline { intensity, speed }
            if (intensity - 0.4).abs() < f32::EPSILON && (speed - 2.0).abs() < f32::EPSILON)
    );

    assert_eq!(scene.effects[5].target.as_deref(), Some("title"));
    assert!(
        matches!(scene.effects[5].kind, EffectKind::Wave { amplitude, frequency, speed }
            if (amplitude - 2.0).abs() < f32::EPSILON
                && (frequency - 0.3).abs() < f32::EPSILON
                && (speed - 1.0).abs() < f32::EPSILON)
    );

    assert_eq!(scene.effects[6].target, None);
    assert!(matches!(scene.effects[6].kind, EffectKind::Dissolve { seed } if seed == plan.seed));

    assert_eq!(scene.effects[7].target.as_deref(), Some("tagline"));
    assert!(matches!(
        scene.effects[7].kind,
        EffectKind::Blink { period: 12 }
    ));
}

#[test]
fn none_effect_consumes_frames_without_creating_an_effect() {
    let mut plan = valid_plan();
    plan.beats = vec![
        Beat {
            effect: PlanEffect::None,
            target: BeatTarget::Title,
            frames: 12,
        },
        Beat {
            effect: PlanEffect::Typewriter {
                chars_per_frame: 1.0,
            },
            target: BeatTarget::Title,
            frames: 8,
        },
    ];

    let scene = compile_plan(&plan).expect("none beat must compile");
    scene.validate().expect("scene must be valid");

    assert_eq!(scene.timeline.frames, 20);
    assert_eq!(scene.effects.len(), 1);
    assert_eq!(scene.effects[0].id, "beat-1");
    assert_eq!((scene.effects[0].start, scene.effects[0].end), (12, 20));
}

#[test]
fn a_single_frame_plan_outputs_png() {
    let mut plan = valid_plan();
    plan.beats = vec![Beat {
        effect: PlanEffect::Typewriter {
            chars_per_frame: 1.0,
        },
        target: BeatTarget::Title,
        frames: 1,
    }];

    let scene = compile_plan(&plan).expect("single frame plan must compile");
    scene.validate().expect("scene must be valid");
    assert_eq!(scene.timeline.frames, 1);
    assert!(matches!(scene.output.format, OutputFormat::Png));
}

#[test]
fn compilation_is_deterministic() {
    let plan = valid_plan();

    let first = compile_plan(&plan).expect("first compile must succeed");
    let second = compile_plan(&plan).expect("second compile must succeed");

    assert_eq!(
        serde_json::to_value(&first).expect("scene must serialize"),
        serde_json::to_value(&second).expect("scene must serialize")
    );
}

#[test]
fn compiler_rejects_an_invalid_plan_with_plan_validation_errors() {
    let mut plan = valid_plan();
    plan.title = "  ".to_string();

    match compile_plan(&plan) {
        Err(CompileError::InvalidPlan(errors)) => {
            assert!(errors.contains(&PlanValidationError::EmptyTitle));
        }
        other => panic!("expected invalid plan errors, got {other:?}"),
    }
}
