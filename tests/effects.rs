//! Behavior tests for the deterministic effects engine (spec S2).
//!
//! Every effect is exercised through the public API only: a fresh grid is
//! built per frame, effects are applied, and the observable cells are checked.

use ascii_forge::effect::{EffectContext, EffectError, apply_canvas, apply_layer};
use ascii_forge::scene::{
    Canvas, Cell, Effect, EffectKind, Grid, GridLayer, Layer, LayerKind, OutputSpec, Rgba,
    SCENE_VERSION, Scene, Timeline, Transform,
};

const FG: Rgba = Rgba::rgb(0, 255, 136);
const BG: Rgba = Rgba::rgb(10, 12, 16);

fn context() -> EffectContext {
    EffectContext {
        scene_seed: 7,
        background: BG,
    }
}

fn make_effect(id: &str, kind: EffectKind, target: Option<&str>, start: u32, end: u32) -> Effect {
    Effect {
        id: id.to_string(),
        kind,
        target: target.map(str::to_string),
        start,
        end,
    }
}

/// A transparent-background layer grid with six opaque glyph cells in row 1.
fn layer_grid() -> Grid {
    let mut grid = Grid::blank(6, 3, FG, Rgba::TRANSPARENT);
    for x in 0..6 {
        grid.set(
            x,
            1,
            Cell {
                ch: 'A',
                fg: FG,
                bg: BG,
            },
        );
    }
    grid
}

/// An opaque canvas grid with six glyph cells in row 1.
fn canvas_grid() -> Grid {
    let mut grid = Grid::blank(6, 3, FG, BG);
    for x in 0..6 {
        grid.set(
            x,
            1,
            Cell {
                ch: 'A',
                fg: FG,
                bg: BG,
            },
        );
    }
    grid
}

fn solid_grid(cols: u16, rows: u16) -> Grid {
    Grid::new(
        cols,
        rows,
        Cell {
            ch: 'X',
            fg: FG,
            bg: BG,
        },
    )
}

fn visible_chars(grid: &Grid) -> usize {
    grid.cells.iter().filter(|cell| cell.ch != ' ').count()
}

fn layer_frame(frame: u32, effects: &[Effect]) -> Grid {
    let mut grid = layer_grid();
    apply_layer(effects, "text", frame, &mut grid, &context()).expect("layer effects apply");
    grid
}

fn canvas_frame(frame: u32, effects: &[Effect]) -> Grid {
    let mut grid = canvas_grid();
    apply_canvas(effects, frame, &mut grid, &context()).expect("canvas effects apply");
    grid
}

fn effect_scene() -> Scene {
    Scene {
        version: SCENE_VERSION,
        seed: 77,
        canvas: Canvas {
            cols: 6,
            rows: 3,
            cell_aspect: 0.542,
            background: BG,
            foreground: FG,
        },
        layers: vec![Layer {
            id: "text".to_string(),
            kind: LayerKind::Grid(GridLayer { grid: layer_grid() }),
            transform: Transform::default(),
            keyframes: Vec::new(),
        }],
        timeline: Timeline {
            fps: 10,
            frames: 8,
            loop_: false,
        },
        effects: vec![
            make_effect(
                "type",
                EffectKind::Typewriter {
                    chars_per_frame: 1.0,
                },
                Some("text"),
                0,
                4,
            ),
            make_effect("fade", EffectKind::Fade { from: 0.2, to: 1.0 }, None, 0, 4),
            make_effect(
                "slide",
                EffectKind::Slide { dx: 2, dy: 1 },
                Some("text"),
                0,
                4,
            ),
            make_effect(
                "scan",
                EffectKind::Scanline {
                    intensity: 0.5,
                    speed: 0.5,
                },
                None,
                0,
                4,
            ),
            make_effect(
                "glitch",
                EffectKind::Glitch {
                    intensity: 0.7,
                    seed: 11,
                },
                None,
                0,
                4,
            ),
            make_effect(
                "wave",
                EffectKind::Wave {
                    amplitude: 1.0,
                    frequency: 0.2,
                    speed: 0.5,
                },
                Some("text"),
                0,
                4,
            ),
            make_effect("dissolve", EffectKind::Dissolve { seed: 5 }, None, 0, 4),
            make_effect("blink", EffectKind::Blink { period: 2 }, Some("text"), 0, 4),
        ],
        output: OutputSpec::default(),
    }
}

#[test]
fn typewriter_reveals_cells_progressively() {
    let effect = make_effect(
        "type",
        EffectKind::Typewriter {
            chars_per_frame: 2.0,
        },
        Some("text"),
        0,
        4,
    );
    let counts: Vec<usize> = (0..4)
        .map(|frame| visible_chars(&layer_frame(frame, std::slice::from_ref(&effect))))
        .collect();

    assert_eq!(counts, [2, 4, 6, 6], "row-major reveal grows per frame");
}

#[test]
fn typewriter_first_active_frame_reveals_at_least_one_cell() {
    let effect = make_effect(
        "slow-type",
        EffectKind::Typewriter {
            chars_per_frame: 0.01,
        },
        Some("text"),
        2,
        5,
    );

    let grid = layer_frame(2, &[effect]);

    assert_eq!(visible_chars(&grid), 1);
    assert_eq!(grid.get(0, 1).unwrap().ch, 'A');
}

#[test]
fn typewriter_hides_unrevealed_cells_keeping_background() {
    let effect = make_effect(
        "type",
        EffectKind::Typewriter {
            chars_per_frame: 2.0,
        },
        Some("text"),
        0,
        4,
    );

    let grid = layer_frame(0, &[effect]);

    assert_eq!(grid.get(1, 1).unwrap().ch, 'A');
    for x in 2..6 {
        let cell = grid.get(x, 1).unwrap();
        assert_eq!(cell.ch, ' ');
        assert_eq!(cell.bg, BG, "hidden cell keeps its background");
    }
}

#[test]
fn fade_blends_toward_background_with_progress() {
    let effect = make_effect("fade", EffectKind::Fade { from: 0.0, to: 1.0 }, None, 0, 4);

    let start = canvas_frame(0, std::slice::from_ref(&effect));
    assert_eq!(start.get(0, 0).unwrap().fg, BG, "progress 0 is fully faded");

    let middle = canvas_frame(2, std::slice::from_ref(&effect));
    assert_eq!(middle.get(0, 0).unwrap().fg, FG.blend(BG, 0.5));
}

#[test]
fn fade_keeps_transparent_layer_cells_transparent() {
    let effect = make_effect(
        "fade",
        EffectKind::Fade { from: 0.0, to: 1.0 },
        Some("text"),
        0,
        4,
    );

    let grid = layer_frame(2, &[effect]);

    assert_eq!(
        grid.get(0, 0).unwrap().bg,
        Rgba::TRANSPARENT,
        "empty layer cells must not gain background"
    );
}

#[test]
fn slide_shifts_content_and_fills_layer_gap_with_transparency() {
    let effect = make_effect(
        "slide",
        EffectKind::Slide { dx: 2, dy: 0 },
        Some("text"),
        0,
        2,
    );

    let grid = layer_frame(1, &[effect]);

    let gap = *grid.get(0, 1).unwrap();
    assert_eq!(gap.ch, ' ');
    assert_eq!(gap.bg, Rgba::TRANSPARENT);
    assert_eq!(grid.get(1, 1).unwrap().ch, 'A');
    assert_eq!(grid.get(5, 1).unwrap().ch, 'A');
}

#[test]
fn slide_fills_canvas_gap_with_background() {
    let effect = make_effect("slide", EffectKind::Slide { dx: 2, dy: 0 }, None, 0, 2);

    let grid = canvas_frame(1, &[effect]);

    let gap = *grid.get(0, 1).unwrap();
    assert_eq!(gap.ch, ' ');
    assert_eq!(gap.bg, BG);
    assert_eq!(grid.get(1, 1).unwrap().ch, 'A');
}

#[test]
fn scanline_moves_and_darkens_a_band() {
    let effect = make_effect(
        "scan",
        EffectKind::Scanline {
            intensity: 1.0,
            speed: 1.0,
        },
        None,
        0,
        20,
    );

    let first = canvas_frame(1, std::slice::from_ref(&effect));
    assert_eq!(
        first.get(0, 1).unwrap().fg,
        BG,
        "band fully darkens its row"
    );

    let second = canvas_frame(2, std::slice::from_ref(&effect));
    assert_eq!(
        second.get(0, 0).unwrap().fg,
        FG,
        "rows outside the band stay"
    );
    assert_eq!(second.get(0, 1).unwrap().fg, FG.blend(BG, 0.5));
    assert_eq!(second.get(0, 2).unwrap().fg, BG);
}

#[test]
fn glitch_is_deterministic_with_a_fixed_seed() {
    let effect = make_effect(
        "glitch",
        EffectKind::Glitch {
            intensity: 1.0,
            seed: 12345,
        },
        None,
        0,
        10,
    );

    let first = canvas_frame(3, std::slice::from_ref(&effect));
    let second = canvas_frame(3, std::slice::from_ref(&effect));

    assert_eq!(first, second);
    assert_ne!(first, canvas_grid(), "strong glitch must disturb the grid");
}

#[test]
fn glitch_with_zero_intensity_is_identity() {
    let effect = make_effect(
        "glitch",
        EffectKind::Glitch {
            intensity: 0.0,
            seed: 12345,
        },
        None,
        0,
        10,
    );

    assert_eq!(canvas_frame(3, &[effect]), canvas_grid());
}

#[test]
fn wave_offsets_columns_vertically() {
    let effect = make_effect(
        "wave",
        EffectKind::Wave {
            amplitude: 2.0,
            frequency: 0.5,
            speed: 0.0,
        },
        None,
        0,
        2,
    );
    let mut grid = Grid::blank(3, 5, FG, BG);
    for (x, ch) in ['0', '1', '2'].into_iter().enumerate() {
        grid.set(x as u16, 2, Cell { ch, fg: FG, bg: BG });
    }

    apply_canvas(&[effect], 0, &mut grid, &context()).expect("wave applies");

    assert_eq!(
        grid.get(0, 2).unwrap().ch,
        '0',
        "zero offset keeps the cell"
    );
    assert_eq!(grid.get(1, 3).unwrap().ch, '1', "rounded sin shifts down");
    assert_eq!(grid.get(1, 2).unwrap().ch, ' ');
    assert_eq!(grid.get(2, 4).unwrap().ch, '2');
}

#[test]
fn dissolve_reveals_progressively_with_a_stable_seed() {
    let effect = make_effect("dissolve", EffectKind::Dissolve { seed: 4242 }, None, 0, 11);
    let solid = solid_grid(10, 10);

    let count = |frame: u32| {
        let mut grid = solid.clone();
        apply_canvas(std::slice::from_ref(&effect), frame, &mut grid, &context())
            .expect("dissolve applies");
        visible_chars(&grid)
    };

    assert_eq!(count(0), 0, "progress 0 hides everything");
    let middle = count(5);
    let late = count(10);
    assert!(
        (1..100).contains(&middle),
        "middle frame shows {middle} cells"
    );
    assert!(
        late > middle,
        "late frame {late} must reveal more than {middle}"
    );

    let mut middle_grid = solid.clone();
    let mut late_grid = solid.clone();
    apply_canvas(
        std::slice::from_ref(&effect),
        5,
        &mut middle_grid,
        &context(),
    )
    .expect("dissolve applies");
    apply_canvas(
        std::slice::from_ref(&effect),
        10,
        &mut late_grid,
        &context(),
    )
    .expect("dissolve applies");
    for (index, (middle_cell, late_cell)) in middle_grid
        .cells
        .iter()
        .zip(late_grid.cells.iter())
        .enumerate()
    {
        if middle_cell.ch != ' ' {
            assert_eq!(late_cell.ch, 'X', "cell {index} flickered off");
        }
    }
}

#[test]
fn blink_toggles_on_period_and_treats_zero_as_visible() {
    let effect = make_effect("blink", EffectKind::Blink { period: 2 }, None, 0, 8);

    assert_eq!(
        visible_chars(&canvas_frame(0, std::slice::from_ref(&effect))),
        6
    );
    assert_eq!(
        visible_chars(&canvas_frame(2, std::slice::from_ref(&effect))),
        0
    );
    assert_eq!(
        visible_chars(&canvas_frame(4, std::slice::from_ref(&effect))),
        6
    );

    let zero = make_effect("blink-zero", EffectKind::Blink { period: 0 }, None, 0, 8);
    assert_eq!(visible_chars(&canvas_frame(3, &[zero])), 6);
}

#[test]
fn effects_are_identity_outside_their_window() {
    let effect = make_effect("slide", EffectKind::Slide { dx: 2, dy: 0 }, None, 3, 5);
    for frame in [0, 2, 5, 9] {
        assert_eq!(
            canvas_frame(frame, std::slice::from_ref(&effect)),
            canvas_grid(),
            "canvas frame {frame} must be untouched"
        );
    }

    let layer_effect = make_effect("blink", EffectKind::Blink { period: 1 }, Some("text"), 3, 5);
    for frame in [0, 2, 5] {
        assert_eq!(
            layer_frame(frame, std::slice::from_ref(&layer_effect)),
            layer_grid(),
            "layer frame {frame} must be untouched"
        );
    }
}

#[test]
fn layer_application_applies_only_matching_targets() {
    let canvas_effect = make_effect("blink", EffectKind::Blink { period: 1 }, None, 0, 10);
    let targeted = make_effect(
        "slide",
        EffectKind::Slide { dx: 2, dy: 0 },
        Some("text"),
        0,
        2,
    );

    let grid = layer_frame(1, &[canvas_effect, targeted]);

    assert_eq!(grid.get(0, 1).unwrap().ch, ' ');
    assert_eq!(grid.get(1, 1).unwrap().ch, 'A');

    let unknown = make_effect(
        "slide",
        EffectKind::Slide { dx: 2, dy: 0 },
        Some("missing"),
        0,
        2,
    );
    assert_eq!(layer_frame(1, &[unknown]), layer_grid());
}

#[test]
fn canvas_application_applies_only_untargeted_effects() {
    let layer_effect = make_effect(
        "blink",
        EffectKind::Blink { period: 1 },
        Some("text"),
        0,
        10,
    );
    assert_eq!(canvas_frame(1, &[layer_effect]), canvas_grid());

    let canvas_effect = make_effect("slide", EffectKind::Slide { dx: 2, dy: 0 }, None, 0, 2);
    let grid = canvas_frame(1, &[canvas_effect]);
    assert_eq!(grid.get(0, 1).unwrap().ch, ' ');
    assert_eq!(grid.get(1, 1).unwrap().ch, 'A');
}

#[test]
fn effects_are_deterministic_for_equal_inputs() {
    let effects = [
        make_effect(
            "glitch",
            EffectKind::Glitch {
                intensity: 0.8,
                seed: 3,
            },
            None,
            0,
            4,
        ),
        make_effect("dissolve", EffectKind::Dissolve { seed: 9 }, None, 0, 4),
        make_effect(
            "wave",
            EffectKind::Wave {
                amplitude: 1.5,
                frequency: 0.3,
                speed: 2.0,
            },
            None,
            0,
            4,
        ),
        make_effect(
            "scan",
            EffectKind::Scanline {
                intensity: 0.6,
                speed: 1.5,
            },
            None,
            0,
            4,
        ),
    ];

    assert_eq!(canvas_frame(2, &effects), canvas_frame(2, &effects));
}

#[test]
fn effects_do_not_mutate_the_scene() {
    let scene = effect_scene();
    let before = serde_json::to_value(&scene).expect("scene serializes");
    let mut grid = canvas_grid();

    for frame in 0..scene.timeline.frames {
        apply_layer(&scene.effects, "text", frame, &mut grid, &context())
            .expect("layer effects apply");
        apply_canvas(&scene.effects, frame, &mut grid, &context()).expect("canvas effects apply");
    }

    let after = serde_json::to_value(&scene).expect("scene serializes");
    assert_eq!(before, after);
}

#[test]
fn non_finite_parameters_return_an_invalid_parameters_error() {
    let not_a_number = make_effect(
        "bad",
        EffectKind::Typewriter {
            chars_per_frame: f32::NAN,
        },
        Some("text"),
        0,
        4,
    );
    let mut grid = layer_grid();
    let error = apply_layer(&[not_a_number], "text", 0, &mut grid, &context())
        .expect_err("NaN is rejected");
    assert!(matches!(error, EffectError::InvalidParameters { .. }));

    let zero = make_effect(
        "zero",
        EffectKind::Typewriter {
            chars_per_frame: 0.0,
        },
        Some("text"),
        0,
        4,
    );
    let mut grid = layer_grid();
    let error = apply_layer(&[zero], "text", 0, &mut grid, &context())
        .expect_err("zero reveal rate is rejected");
    assert!(matches!(error, EffectError::InvalidParameters { .. }));
}

#[test]
fn tiny_grids_do_not_panic() {
    let kinds = [
        EffectKind::Typewriter {
            chars_per_frame: 1.0,
        },
        EffectKind::Fade { from: 0.0, to: 1.0 },
        EffectKind::Slide { dx: 4, dy: -3 },
        EffectKind::Scanline {
            intensity: 1.0,
            speed: 2.0,
        },
        EffectKind::Glitch {
            intensity: 1.0,
            seed: 8,
        },
        EffectKind::Wave {
            amplitude: 4.0,
            frequency: 2.0,
            speed: 1.0,
        },
        EffectKind::Dissolve { seed: 2 },
        EffectKind::Blink { period: 2 },
    ];

    for (index, kind) in kinds.into_iter().enumerate() {
        let id = format!("tiny-{index}");
        for frame in 0..6 {
            let mut layer = Grid::new(
                1,
                1,
                Cell {
                    ch: 'X',
                    fg: FG,
                    bg: Rgba::TRANSPARENT,
                },
            );
            let layer_effect = make_effect(&id, kind.clone(), Some("text"), 0, 6);
            apply_layer(
                std::slice::from_ref(&layer_effect),
                "text",
                frame,
                &mut layer,
                &context(),
            )
            .expect("tiny layer effects apply");

            let mut canvas = Grid::new(
                1,
                1,
                Cell {
                    ch: 'X',
                    fg: FG,
                    bg: BG,
                },
            );
            let canvas_effect = make_effect(&id, kind.clone(), None, 0, 6);
            apply_canvas(
                std::slice::from_ref(&canvas_effect),
                frame,
                &mut canvas,
                &context(),
            )
            .expect("tiny canvas effects apply");
        }
    }
}
