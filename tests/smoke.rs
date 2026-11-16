use ascii_forge::scene::{Canvas, OutputSpec, Rgba, SCENE_VERSION, Scene, Timeline};

#[test]
fn scene_round_trips_through_json() {
    let scene = Scene {
        version: SCENE_VERSION,
        seed: 42,
        canvas: Canvas {
            cols: 40,
            rows: 12,
            cell_aspect: 0.542,
            background: Rgba::rgb(10, 12, 16),
            foreground: Rgba::rgb(0, 255, 136),
        },
        layers: Vec::new(),
        timeline: Timeline {
            fps: 30,
            frames: 1,
            loop_: false,
        },
        effects: Vec::new(),
        output: OutputSpec::default(),
    };

    let json = serde_json::to_string(&scene).expect("serialize scene");
    let restored: Scene = serde_json::from_str(&json).expect("deserialize scene");

    assert_eq!(restored.version, SCENE_VERSION);
    assert_eq!(restored.canvas.cols, 40);
    assert_eq!(restored.canvas.rows, 12);
    assert_eq!(restored.timeline.fps, 30);
    assert_eq!(restored.timeline.frames, 1);
    assert!(!restored.timeline.loop_);
}
