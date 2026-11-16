//! Behavior tests for timeline interpolation and easing (spec S1).

use ascii_forge::scene::{Align, Easing, Keyframe, Layer, LayerKind, TextLayer, Transform};
use ascii_forge::timeline::{ease, interpolate, transform_at};

fn transform(x: i32, y: i32, scale: f32, opacity: f32) -> Transform {
    Transform {
        x,
        y,
        scale,
        opacity,
    }
}

fn keyframe(frame: u32, transform: Transform, easing: Easing) -> Keyframe {
    Keyframe {
        frame,
        transform,
        easing,
    }
}

fn layer(transform: Transform, keyframes: Vec<Keyframe>) -> Layer {
    Layer {
        id: "layer".to_string(),
        kind: LayerKind::Text(TextLayer {
            text: "x".to_string(),
            color: None,
            align: Align::Left,
        }),
        transform,
        keyframes,
    }
}

#[test]
fn no_keyframes_returns_the_base_transform() {
    let base = transform(3, -4, 1.5, 0.75);
    let layer = layer(base, Vec::new());

    assert_eq!(transform_at(&layer, 0), base);
    assert_eq!(transform_at(&layer, 42), base);
}

#[test]
fn midpoint_between_two_keyframes_interpolates_every_field() {
    let from = transform(0, 0, 1.0, 1.0);
    let to = transform(10, 20, 2.0, 0.0);
    let layer = layer(
        Transform::default(),
        vec![
            keyframe(0, from, Easing::Linear),
            keyframe(10, to, Easing::Linear),
        ],
    );

    assert_eq!(transform_at(&layer, 5), transform(5, 10, 1.5, 0.5));
}

#[test]
fn interpolated_offsets_round_to_the_nearest_integer() {
    let layer = layer(
        Transform::default(),
        vec![
            keyframe(0, transform(0, 0, 1.0, 1.0), Easing::Linear),
            keyframe(10, transform(5, 7, 1.0, 1.0), Easing::Linear),
        ],
    );

    // 0..5 at progress 0.5 is 2.5; 0..7 at progress 0.5 is 3.5.
    assert_eq!(transform_at(&layer, 5), transform(3, 4, 1.0, 1.0));
}

#[test]
fn frames_outside_the_keyframe_range_clamp_to_the_endpoints() {
    let first = transform(1, 2, 1.0, 1.0);
    let last = transform(9, 8, 3.0, 0.25);
    let layer = layer(
        Transform::default(),
        vec![
            keyframe(10, first, Easing::Linear),
            keyframe(20, last, Easing::Linear),
        ],
    );

    assert_eq!(transform_at(&layer, 0), first);
    assert_eq!(transform_at(&layer, 9), first);
    assert_eq!(transform_at(&layer, 10), first);
    assert_eq!(transform_at(&layer, 20), last);
    assert_eq!(transform_at(&layer, 21), last);
    assert_eq!(transform_at(&layer, 10_000), last);
}

#[test]
fn exact_frame_returns_that_keyframe() {
    let first = transform(0, 0, 1.0, 1.0);
    let middle = transform(4, 6, 1.5, 0.8);
    let last = transform(8, 12, 2.0, 0.6);
    let layer = layer(
        Transform::default(),
        vec![
            keyframe(0, first, Easing::EaseIn),
            keyframe(10, middle, Easing::EaseOut),
            keyframe(20, last, Easing::Linear),
        ],
    );

    assert_eq!(transform_at(&layer, 0), first);
    assert_eq!(transform_at(&layer, 10), middle);
    assert_eq!(transform_at(&layer, 20), last);
}

#[test]
fn interpolation_uses_the_source_keyframe_easing() {
    let from = transform(0, 0, 1.0, 1.0);
    let to = transform(100, 100, 1.0, 1.0);
    let linear_source = layer(
        Transform::default(),
        vec![
            keyframe(0, from, Easing::Linear),
            keyframe(10, to, Easing::EaseIn),
        ],
    );
    let eased_source = layer(
        Transform::default(),
        vec![
            keyframe(0, from, Easing::EaseIn),
            keyframe(10, to, Easing::Linear),
        ],
    );

    assert_eq!(transform_at(&linear_source, 5), transform(50, 50, 1.0, 1.0));
    assert_eq!(transform_at(&eased_source, 5), transform(25, 25, 1.0, 1.0));
}

#[test]
fn standard_easings_match_their_curves() {
    for easing in [
        Easing::Linear,
        Easing::EaseIn,
        Easing::EaseOut,
        Easing::EaseInOut,
    ] {
        assert_eq!(ease(easing, 0.0), 0.0, "{easing:?} at 0");
        assert_eq!(ease(easing, 1.0), 1.0, "{easing:?} at 1");
    }

    assert_eq!(ease(Easing::Linear, 0.5), 0.5);
    assert_eq!(ease(Easing::EaseIn, 0.5), 0.25);
    assert_eq!(ease(Easing::EaseOut, 0.5), 0.75);
    assert_eq!(ease(Easing::EaseInOut, 0.5), 0.5);
    assert_eq!(ease(Easing::EaseIn, 0.25), 0.0625);
    assert_eq!(ease(Easing::EaseOut, 0.25), 0.4375);
    assert_eq!(ease(Easing::EaseInOut, 0.25), 0.125);
    assert_eq!(ease(Easing::EaseInOut, 0.75), 0.875);
}

#[test]
fn ease_and_interpolate_clamp_the_progress() {
    assert_eq!(ease(Easing::Linear, -0.5), 0.0);
    assert_eq!(ease(Easing::Linear, 1.5), 1.0);

    let from = transform(0, 0, 1.0, 1.0);
    let to = transform(10, 20, 2.0, 0.0);
    assert_eq!(interpolate(from, to, -1.0), from);
    assert_eq!(interpolate(from, to, 2.0), to);
    assert_eq!(interpolate(from, to, 0.0), from);
    assert_eq!(interpolate(from, to, 1.0), to);
}

#[test]
fn cubic_bezier_hits_the_endpoints_and_the_symmetric_midpoint() {
    let ease_in_out = Easing::CubicBezier {
        x1: 0.42,
        y1: 0.0,
        x2: 0.58,
        y2: 1.0,
    };

    assert_eq!(ease(ease_in_out, 0.0), 0.0);
    assert_eq!(ease(ease_in_out, 1.0), 1.0);
    let middle = ease(ease_in_out, 0.5);
    assert!((middle - 0.5).abs() < 1e-4, "expected 0.5, got {middle}");
    assert!(ease(ease_in_out, 0.25) < middle);
    assert!(ease(ease_in_out, 0.75) > middle);
}

#[test]
fn cubic_bezier_keeps_out_of_range_controls_inside_the_unit_interval() {
    let awkward = Easing::CubicBezier {
        x1: -2.0,
        y1: 2.0,
        x2: 3.0,
        y2: -1.0,
    };

    for step in 0..=10 {
        let value = ease(awkward, step as f32 / 10.0);
        assert!(value.is_finite(), "expected a finite value, got {value}");
        assert!((0.0..=1.0).contains(&value), "out of range: {value}");
    }
}

#[test]
fn malformed_keyframes_do_not_panic() {
    let unsorted = vec![
        keyframe(10, transform(1, 1, 1.0, 1.0), Easing::Linear),
        keyframe(5, transform(2, 2, 1.0, 1.0), Easing::Linear),
    ];
    let duplicated = vec![
        keyframe(5, transform(1, 1, 1.0, 1.0), Easing::EaseIn),
        keyframe(5, transform(2, 2, 1.0, 1.0), Easing::EaseOut),
    ];

    for keyframes in [unsorted, duplicated] {
        for frame in [0, 5, 7, 10, 20] {
            let layer = layer(Transform::default(), keyframes.clone());
            let first = transform_at(&layer, frame);
            assert_eq!(first, transform_at(&layer, frame));
        }
    }
}

#[test]
fn repeated_calls_are_identical() {
    let layer = layer(
        Transform::default(),
        vec![
            keyframe(0, transform(0, 0, 1.0, 1.0), Easing::EaseInOut),
            keyframe(
                7,
                transform(13, -5, 1.75, 0.3),
                Easing::CubicBezier {
                    x1: 0.42,
                    y1: 0.0,
                    x2: 0.58,
                    y2: 1.0,
                },
            ),
            keyframe(15, transform(-3, 9, 0.5, 0.9), Easing::EaseOut),
        ],
    );

    for frame in 0..20 {
        let first = transform_at(&layer, frame);
        assert_eq!(first, transform_at(&layer, frame));
        assert_eq!(first, transform_at(&layer, frame));
    }
}
