//! Behavior tests for imported video frame layers (spec S4).

use ascii_forge::scene::compose::{ComposeError, compose_frame};
use ascii_forge::scene::{
    Canvas, Cell, FramesLayer, Grid, GridLayer, Layer, LayerKind, OutputSpec, Rgba, SCENE_VERSION,
    Scene, Timeline, Transform, ValidationError,
};

fn cell(ch: char) -> Cell {
    Cell {
        ch,
        fg: Rgba::rgb(255, 255, 255),
        bg: Rgba::TRANSPARENT,
    }
}

fn grid(ch: char, cols: u16, rows: u16) -> Grid {
    Grid::new(cols, rows, cell(ch))
}

fn frames_layer(id: &str, frames: Vec<Grid>) -> Layer {
    Layer {
        id: id.to_string(),
        kind: LayerKind::Frames(FramesLayer { frames }),
        transform: Transform::default(),
        keyframes: Vec::new(),
    }
}

fn scene_with(layers: Vec<Layer>, frames: u32) -> Scene {
    Scene {
        version: SCENE_VERSION,
        seed: 1,
        canvas: Canvas {
            cols: 4,
            rows: 2,
            cell_aspect: 0.542,
            background: Rgba::rgb(0, 0, 0),
            foreground: Rgba::rgb(255, 255, 255),
        },
        layers,
        timeline: Timeline {
            fps: 12,
            frames,
            loop_: true,
        },
        effects: Vec::new(),
        output: OutputSpec::default(),
    }
}

fn errors_of(scene: &Scene) -> Vec<ValidationError> {
    scene.validate().expect_err("scene should be invalid")
}

#[test]
fn frames_layer_round_trips_through_json() {
    let scene = scene_with(
        vec![frames_layer(
            "video",
            vec![grid('A', 2, 2), grid('B', 2, 2)],
        )],
        2,
    );

    let json = serde_json::to_string(&scene).expect("scene must serialize");
    assert!(json.contains("\"type\":\"frames\""), "json: {json}");

    let decoded: Scene = serde_json::from_str(&json).expect("scene must deserialize");
    assert_eq!(
        serde_json::to_value(&decoded).expect("decoded scene must serialize"),
        serde_json::to_value(&scene).expect("scene must serialize"),
    );

    match &decoded.layers[0].kind {
        LayerKind::Frames(frames) => {
            assert_eq!(frames.frames.len(), 2);
            assert_eq!(frames.frames[0].to_text(), "AA\nAA\n");
            assert_eq!(frames.frames[1].to_text(), "BB\nBB\n");
        }
        other => panic!("layer must be a frames layer, got {other:?}"),
    }
}

#[test]
fn compose_selects_frames_by_modulo() {
    let layer = frames_layer("video", vec![grid('A', 1, 1), grid('B', 1, 1)]);
    let scene = scene_with(vec![layer], 3);

    let first = compose_frame(&scene, 0).expect("frame 0");
    let second = compose_frame(&scene, 1).expect("frame 1");
    let third = compose_frame(&scene, 2).expect("frame 2");

    assert_eq!(first.grid.get(0, 0).map(|cell| cell.ch), Some('A'));
    assert_eq!(second.grid.get(0, 0).map(|cell| cell.ch), Some('B'));
    assert_eq!(third.grid.get(0, 0).map(|cell| cell.ch), Some('A'));
}

#[test]
fn compose_rejects_a_layer_without_frames() {
    let scene = scene_with(vec![frames_layer("video", Vec::new())], 1);

    let error = compose_frame(&scene, 0).expect_err("empty frames layer must not compose");
    assert!(
        matches!(&error, ComposeError::EmptyFrames { layer } if layer == "video"),
        "unexpected error: {error}"
    );
    assert!(error.to_string().contains("video"), "message: {error}");
}

#[test]
fn rejects_a_layer_without_frames() {
    let scene = scene_with(vec![frames_layer("video", Vec::new())], 1);

    let errors = errors_of(&scene);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(
        matches!(&errors[0], ValidationError::EmptyFrames { layer } if layer == "video"),
        "unexpected error: {}",
        errors[0]
    );
}

#[test]
fn rejects_more_than_one_thousand_frames() {
    let too_many = scene_with(vec![frames_layer("video", vec![grid('A', 1, 1); 1001])], 1);
    let errors = errors_of(&too_many);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(
        matches!(
            &errors[0],
            ValidationError::FramesTooMany {
                layer,
                count: 1001,
                max: 1000,
            } if layer == "video"
        ),
        "unexpected error: {}",
        errors[0]
    );

    let at_limit = scene_with(vec![frames_layer("video", vec![grid('A', 1, 1); 1000])], 1);
    assert_eq!(at_limit.validate(), Ok(()));
}

#[test]
fn rejects_zero_sized_frame_grids() {
    let scene = scene_with(
        vec![frames_layer(
            "video",
            vec![Grid::new(0, 2, cell('A')), Grid::new(2, 0, cell('A'))],
        )],
        1,
    );

    let errors = errors_of(&scene);
    assert_eq!(errors.len(), 2, "{errors:?}");
    assert!(
        matches!(
            &errors[0],
            ValidationError::FrameGridEmpty {
                layer,
                frame: 0,
                cols: 0,
                rows: 2,
            } if layer == "video"
        ),
        "unexpected error: {}",
        errors[0]
    );
    assert!(
        matches!(
            &errors[1],
            ValidationError::FrameGridEmpty {
                layer,
                frame: 1,
                cols: 2,
                rows: 0,
            } if layer == "video"
        ),
        "unexpected error: {}",
        errors[1]
    );
}

#[test]
fn rejects_frame_grids_with_inconsistent_cell_counts() {
    let broken = Grid {
        cols: 2,
        rows: 2,
        cells: vec![cell('A'); 3],
    };
    let scene = scene_with(
        vec![frames_layer("video", vec![grid('A', 1, 1), broken])],
        1,
    );

    let errors = errors_of(&scene);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(
        matches!(
            &errors[0],
            ValidationError::FrameGridCellCount {
                layer,
                frame: 1,
                cols: 2,
                rows: 2,
                cells: 3,
                expected: 4,
            } if layer == "video"
        ),
        "unexpected error: {}",
        errors[0]
    );
}

#[test]
fn grid_layers_still_compose_unchanged() {
    let layer = Layer {
        id: "static".to_string(),
        kind: LayerKind::Grid(GridLayer {
            grid: Grid {
                cols: 2,
                rows: 1,
                cells: vec![cell('X'), cell('Y')],
            },
        }),
        transform: Transform::default(),
        keyframes: Vec::new(),
    };
    let scene = scene_with(vec![layer], 1);

    let frame = compose_frame(&scene, 0).expect("grid frame");
    assert_eq!(frame.grid.get(0, 0).map(|cell| cell.ch), Some('X'));
    assert_eq!(frame.grid.get(1, 0).map(|cell| cell.ch), Some('Y'));
    assert_eq!(frame.grid.get(2, 0).map(|cell| cell.ch), Some(' '));
}

#[test]
fn scene_schema_includes_the_frames_variant() {
    let schema = schemars::schema_for!(Scene);
    let json = serde_json::to_value(&schema).expect("schema must serialize to JSON");
    let rendered = json.to_string();

    assert!(
        rendered.contains("\"frames\""),
        "schema must describe the frames variant: {rendered}"
    );
    assert!(
        rendered.contains("FramesLayer"),
        "schema must define FramesLayer: {rendered}"
    );
}
