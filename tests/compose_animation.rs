//! Integration tests for animated composition: keyframes, transforms and effects.

use ascii_forge::scene::compose::compose_frame;
use ascii_forge::scene::{
    Align, Canvas, Cell, Easing, Effect, EffectKind, Grid, GridLayer, Keyframe, Layer, LayerKind,
    OutputSpec, Rgba, SCENE_VERSION, Scene, TextLayer, Timeline, Transform,
};

fn canvas(cols: u16, rows: u16) -> Canvas {
    Canvas {
        cols,
        rows,
        cell_aspect: 0.542,
        background: Rgba::rgb(0, 0, 0),
        foreground: Rgba::rgb(255, 255, 255),
    }
}

fn grid_layer(id: &str, ch: char, cols: u16, rows: u16) -> Layer {
    Layer {
        id: id.to_string(),
        kind: LayerKind::Grid(GridLayer {
            grid: Grid::new(
                cols,
                rows,
                Cell {
                    ch,
                    fg: Rgba::rgb(255, 255, 255),
                    bg: Rgba::TRANSPARENT,
                },
            ),
        }),
        transform: Transform::default(),
        keyframes: Vec::new(),
    }
}

fn scene_with(layers: Vec<Layer>, frames: u32) -> Scene {
    Scene {
        version: SCENE_VERSION,
        seed: 42,
        canvas: Canvas {
            cols: 10,
            rows: 5,
            ..canvas(10, 5)
        },
        layers,
        timeline: Timeline {
            fps: 30,
            frames,
            loop_: false,
        },
        effects: Vec::new(),
        output: OutputSpec::default(),
    }
}

fn count(grid: &Grid, ch: char) -> usize {
    grid.cells.iter().filter(|cell| cell.ch == ch).count()
}

#[test]
fn keyframes_move_a_layer_between_frames() {
    let mut layer = grid_layer("cursor", 'X', 1, 1);
    layer.keyframes = vec![
        Keyframe {
            frame: 0,
            transform: Transform {
                x: 0,
                y: 0,
                scale: 1.0,
                opacity: 1.0,
            },
            easing: Easing::Linear,
        },
        Keyframe {
            frame: 10,
            transform: Transform {
                x: 5,
                y: 0,
                scale: 1.0,
                opacity: 1.0,
            },
            easing: Easing::Linear,
        },
    ];
    let scene = scene_with(vec![layer], 11);

    let start = compose_frame(&scene, 0).expect("frame 0");
    let middle = compose_frame(&scene, 5).expect("frame 5");
    let end = compose_frame(&scene, 10).expect("frame 10");

    assert_eq!(start.grid.get(0, 0).map(|cell| cell.ch), Some('X'));
    assert_eq!(middle.grid.get(3, 0).map(|cell| cell.ch), Some('X'));
    assert_eq!(end.grid.get(5, 0).map(|cell| cell.ch), Some('X'));
}

#[test]
fn keyframe_easing_changes_intermediate_position() {
    let mut layer = grid_layer("cursor", 'X', 1, 1);
    layer.keyframes = vec![
        Keyframe {
            frame: 0,
            transform: Transform::default(),
            easing: Easing::EaseIn,
        },
        Keyframe {
            frame: 10,
            transform: Transform {
                x: 10,
                y: 0,
                scale: 1.0,
                opacity: 1.0,
            },
            easing: Easing::Linear,
        },
    ];
    let scene = scene_with(vec![layer], 11);

    let middle = compose_frame(&scene, 5).expect("frame 5");
    // Ease-in at 0.5 is 0.25, so x is 2.5 rounded to 3.
    assert_eq!(middle.grid.get(3, 0).map(|cell| cell.ch), Some('X'));
    assert_eq!(middle.grid.get(5, 0).map(|cell| cell.ch), Some(' '));
}

#[test]
fn layer_effect_only_touches_its_target() {
    let target = grid_layer("target", 'X', 3, 1);
    let mut other = grid_layer("other", 'Y', 3, 1);
    other.transform.x = 5;
    let mut scene = scene_with(vec![target, other], 10);
    scene.effects = vec![Effect {
        id: "reveal".to_string(),
        kind: EffectKind::Typewriter {
            chars_per_frame: 1.0,
        },
        target: Some("target".to_string()),
        start: 0,
        end: 10,
    }];

    let frame = compose_frame(&scene, 0).expect("frame 0");
    assert_eq!(count(&frame.grid, 'X'), 1);
    assert_eq!(count(&frame.grid, 'Y'), 3);
}

#[test]
fn canvas_fade_blends_the_composed_frame() {
    let layer = grid_layer("text", 'X', 1, 1);
    let mut scene = scene_with(vec![layer], 10);
    scene.effects = vec![Effect {
        id: "fade".to_string(),
        kind: EffectKind::Fade { from: 0.0, to: 1.0 },
        target: None,
        start: 0,
        end: 10,
    }];

    let frame = compose_frame(&scene, 0).expect("frame 0");
    let cell = frame.grid.get(0, 0).expect("cell");
    assert_eq!(cell.ch, 'X');
    assert_eq!(cell.fg, scene.canvas.background);
}

#[test]
fn scale_doubles_the_layer_grid() {
    let mut layer = grid_layer("block", 'X', 1, 1);
    layer.transform.scale = 2.0;
    let scene = scene_with(vec![layer], 1);

    let frame = compose_frame(&scene, 0).expect("frame 0");
    assert_eq!(count(&frame.grid, 'X'), 4);
}

#[test]
fn opacity_zero_hides_the_layer() {
    let mut layer = grid_layer("ghost", 'X', 1, 1);
    layer.transform.opacity = 0.0;
    let scene = scene_with(vec![layer], 1);

    let frame = compose_frame(&scene, 0).expect("frame 0");
    assert_eq!(count(&frame.grid, 'X'), 0);
}

#[test]
fn deterministic_effects_produce_identical_frames() {
    let layer = grid_layer("text", 'X', 2, 2);
    let mut scene = scene_with(vec![layer], 10);
    scene.effects = vec![Effect {
        id: "glitch".to_string(),
        kind: EffectKind::Glitch {
            intensity: 0.5,
            seed: 9,
        },
        target: None,
        start: 0,
        end: 10,
    }];

    let first = compose_frame(&scene, 3).expect("frame 3");
    let second = compose_frame(&scene, 3).expect("frame 3 again");
    assert_eq!(first.grid, second.grid);
}

#[test]
fn text_layers_still_compose_with_the_animation_pipeline() {
    let layer = Layer {
        id: "title".to_string(),
        kind: LayerKind::Text(TextLayer {
            text: "HI".to_string(),
            color: None,
            align: Align::Center,
        }),
        transform: Transform::default(),
        keyframes: Vec::new(),
    };
    let scene = scene_with(vec![layer], 1);

    let frame = compose_frame(&scene, 0).expect("text frame");
    assert!(frame.grid.to_text().contains('H'));
}
