use ascii_forge::ai::plan::PlanValidationError;
use ascii_forge::ai::{
    Aspect, BeatTarget, Layout, PlanEffect, ScenePlan, TemplateKind, compile_plan,
};
use ascii_forge::scene::{Align, LayerKind, Rgba};
use ascii_forge::templates::{TemplateError, TemplateParams, build};

const KINDS: [TemplateKind; 5] = [
    TemplateKind::Logo,
    TemplateKind::Promo,
    TemplateKind::Glitch,
    TemplateKind::Matrix,
    TemplateKind::LowerThird,
];

const DARK: Rgba = Rgba::rgb(10, 12, 16);
const BLACK: Rgba = Rgba::rgb(0, 0, 0);
const WHITE: Rgba = Rgba::rgb(255, 255, 255);
const CYAN: Rgba = Rgba::rgb(0, 255, 255);
const MAGENTA: Rgba = Rgba::rgb(255, 0, 255);
const GREEN: Rgba = Rgba::rgb(0, 255, 136);

fn params() -> TemplateParams {
    TemplateParams {
        title: "ASCII Forge".to_string(),
        tagline: Some("Build scenes from text".to_string()),
        seed: 7,
    }
}

fn build_ok(kind: TemplateKind, params: &TemplateParams) -> ScenePlan {
    build(kind, params).unwrap_or_else(|error| panic!("{kind:?}: {error}"))
}

fn target_counts(plan: &ScenePlan) -> (usize, usize, usize) {
    plan.beats.iter().fold(
        (0_usize, 0_usize, 0_usize),
        |(title, tagline, canvas), beat| match beat.target {
            BeatTarget::Title => (title + 1, tagline, canvas),
            BeatTarget::Tagline => (title, tagline + 1, canvas),
            BeatTarget::Canvas => (title, tagline, canvas + 1),
        },
    )
}

#[test]
fn every_template_builds_to_a_plan_that_compiles_to_a_valid_scene() {
    for kind in KINDS {
        let plan = build_ok(kind, &params());
        plan.validate()
            .unwrap_or_else(|errors| panic!("{kind:?}: {errors:?}"));

        let scene = compile_plan(&plan).unwrap_or_else(|error| panic!("{kind:?}: {error}"));
        scene
            .validate()
            .unwrap_or_else(|errors| panic!("{kind:?}: {errors:?}"));

        assert_eq!(plan.template, kind);
        let (width, height) = plan.aspect.dimensions();
        assert_eq!((scene.output.width, scene.output.height), (width, height));
        assert_eq!(scene.seed, params().seed);
        assert_eq!(scene.layers[0].id, "title");
    }
}

#[test]
fn templates_reject_blank_titles() {
    for kind in KINDS {
        for blank in ["", "   ", "\t\n"] {
            let mut params = params();
            params.title = blank.to_string();
            assert!(
                matches!(build(kind, &params), Err(TemplateError::BlankTitle)),
                "{kind:?} must reject a blank title"
            );
        }
    }
}

#[test]
fn templates_reject_titles_longer_than_the_plan_limit() {
    let mut params = params();
    params.title = "x".repeat(81);

    for kind in KINDS {
        match build(kind, &params) {
            Err(TemplateError::InvalidPlan(errors)) => {
                assert!(errors.contains(&PlanValidationError::TitleTooLong { len: 81, max: 80 }));
            }
            other => panic!("{kind:?} must reject an overlong title, got {other:?}"),
        }
    }
}

#[test]
fn templates_reject_taglines_longer_than_the_plan_limit() {
    let mut params = params();
    params.tagline = Some("x".repeat(121));

    for kind in KINDS {
        match build(kind, &params) {
            Err(TemplateError::InvalidPlan(errors)) => {
                assert!(
                    errors.contains(&PlanValidationError::TaglineTooLong { len: 121, max: 120 })
                );
            }
            other => panic!("{kind:?} must reject an overlong tagline, got {other:?}"),
        }
    }
}

#[test]
fn missing_tagline_still_produces_a_valid_plan_and_scene() {
    let mut params = params();
    params.tagline = None;

    for kind in KINDS {
        let plan = build_ok(kind, &params);
        assert_eq!(plan.tagline, None);
        plan.validate()
            .unwrap_or_else(|errors| panic!("{kind:?}: {errors:?}"));

        let scene = compile_plan(&plan).unwrap_or_else(|error| panic!("{kind:?}: {error}"));
        scene
            .validate()
            .unwrap_or_else(|errors| panic!("{kind:?}: {errors:?}"));

        assert_eq!(target_counts(&plan).1, 0);
        assert!(!scene.layers.iter().any(|layer| layer.id == "tagline"));
    }
}

#[test]
fn blank_tagline_is_treated_as_absent() {
    let mut params = params();
    params.tagline = Some("   ".to_string());

    for kind in KINDS {
        let plan = build_ok(kind, &params);
        assert_eq!(plan.tagline, None);
        plan.validate()
            .unwrap_or_else(|errors| panic!("{kind:?}: {errors:?}"));
        assert_eq!(target_counts(&plan).1, 0);
    }
}

#[test]
fn template_shapes_match_the_documented_designs() {
    let logo = build_ok(TemplateKind::Logo, &params());
    assert_eq!(logo.aspect, Aspect::Landscape);
    assert_eq!(logo.layout, Layout::Centered);
    assert_eq!(logo.font, "standard");
    assert_eq!(logo.palette, vec![DARK, CYAN, WHITE]);
    assert_eq!(logo.beats.len(), 2);
    assert!(matches!(
        logo.beats[0].effect,
        PlanEffect::Typewriter { .. }
    ));
    assert_eq!(logo.beats[0].target, BeatTarget::Title);
    assert!(matches!(logo.beats[1].effect, PlanEffect::Fade { .. }));
    assert_eq!(logo.beats[1].target, BeatTarget::Title);

    let promo = build_ok(TemplateKind::Promo, &params());
    assert_eq!(promo.aspect, Aspect::Landscape);
    assert_eq!(promo.layout, Layout::Stacked);
    assert_eq!(promo.font, "standard");
    assert_eq!(promo.palette, vec![DARK, MAGENTA, WHITE]);
    assert_eq!(promo.beats.len(), 3);
    assert!(matches!(promo.beats[0].effect, PlanEffect::Slide { .. }));
    assert_eq!(promo.beats[0].target, BeatTarget::Title);
    assert!(matches!(promo.beats[1].effect, PlanEffect::Fade { .. }));
    assert_eq!(promo.beats[1].target, BeatTarget::Tagline);
    assert!(matches!(
        promo.beats[2].effect,
        PlanEffect::Typewriter { .. }
    ));
    assert_eq!(promo.beats[2].target, BeatTarget::Title);

    let glitch = build_ok(TemplateKind::Glitch, &params());
    assert_eq!(glitch.aspect, Aspect::Landscape);
    assert_eq!(glitch.layout, Layout::FullBleed);
    assert_eq!(glitch.font, "standard");
    assert_eq!(glitch.palette, vec![BLACK, GREEN, GREEN]);
    assert_eq!(target_counts(&glitch), (0, 0, 2));
    assert!(matches!(glitch.beats[0].effect, PlanEffect::Glitch { .. }));
    assert!(matches!(
        glitch.beats[1].effect,
        PlanEffect::Scanline { .. }
    ));

    let matrix = build_ok(TemplateKind::Matrix, &params());
    assert_eq!(matrix.aspect, Aspect::Portrait);
    assert_eq!(matrix.layout, Layout::FullBleed);
    assert_eq!(matrix.font, "slant");
    assert_eq!(matrix.palette, vec![BLACK, GREEN, GREEN]);
    assert_eq!(matrix.beats.len(), 2);
    assert!(matches!(matrix.beats[0].effect, PlanEffect::Dissolve));
    assert_eq!(matrix.beats[0].target, BeatTarget::Title);
    assert!(matches!(
        matrix.beats[1].effect,
        PlanEffect::Scanline { .. }
    ));
    assert_eq!(matrix.beats[1].target, BeatTarget::Canvas);

    let lower_third = build_ok(TemplateKind::LowerThird, &params());
    assert_eq!(lower_third.aspect, Aspect::Landscape);
    assert_eq!(lower_third.layout, Layout::LowerThird);
    assert_eq!(lower_third.font, "standard");
    assert_eq!(lower_third.palette, vec![DARK, WHITE, CYAN]);
    assert_eq!(lower_third.beats.len(), 2);
    assert!(matches!(
        lower_third.beats[0].effect,
        PlanEffect::Slide { .. }
    ));
    assert_eq!(lower_third.beats[0].target, BeatTarget::Title);
    assert!(matches!(
        lower_third.beats[1].effect,
        PlanEffect::Typewriter { .. }
    ));
    assert_eq!(lower_third.beats[1].target, BeatTarget::Tagline);
}

#[test]
fn promo_and_lower_third_skip_the_tagline_beat_when_absent() {
    let mut without_tagline = params();
    without_tagline.tagline = None;

    for kind in [TemplateKind::Promo, TemplateKind::LowerThird] {
        let plan = build_ok(kind, &without_tagline);
        assert_eq!(target_counts(&plan).1, 0);

        let plan = build_ok(kind, &params());
        assert_eq!(target_counts(&plan).1, 1);
    }
}

#[test]
fn template_plans_use_the_plan_alignment_for_the_tagline() {
    for kind in KINDS {
        let plan = build_ok(kind, &params());
        assert_eq!(plan.align, Align::Center);

        let scene = compile_plan(&plan).expect("template plan must compile");
        let tagline = scene
            .layers
            .iter()
            .find(|layer| layer.id == "tagline")
            .expect("template scene must have a tagline layer");
        match &tagline.kind {
            LayerKind::Text(text) => assert_eq!(text.align, plan.align),
            other => panic!("{kind:?}: tagline must be a text layer, got {other:?}"),
        }
    }
}

#[test]
fn logo_plan_snapshot() {
    let plan = build_ok(TemplateKind::Logo, &params());
    insta::assert_json_snapshot!(plan);
}

#[test]
fn promo_plan_snapshot() {
    let plan = build_ok(TemplateKind::Promo, &params());
    insta::assert_json_snapshot!(plan);
}

#[test]
fn glitch_plan_snapshot() {
    let plan = build_ok(TemplateKind::Glitch, &params());
    insta::assert_json_snapshot!(plan);
}

#[test]
fn matrix_plan_snapshot() {
    let plan = build_ok(TemplateKind::Matrix, &params());
    insta::assert_json_snapshot!(plan);
}

#[test]
fn lower_third_plan_snapshot() {
    let plan = build_ok(TemplateKind::LowerThird, &params());
    insta::assert_json_snapshot!(plan);
}
