//! Scene validation (spec S1).
//!
//! [`validate`] collects every problem it finds so callers can report a full
//! list instead of stopping at the first error.

use std::collections::HashSet;

use super::{
    Canvas, Easing, Effect, EffectKind, Grid, Layer, LayerKind, OutputFormat, OutputSpec,
    SCENE_VERSION, Scene, Timeline, Transform,
};
use thiserror::Error;

const MIN_CANVAS_DIMENSION: u16 = 1;
const MAX_CANVAS_DIMENSION: u16 = 512;
const MIN_CELL_ASPECT: f32 = 0.2;
const MAX_CELL_ASPECT: f32 = 2.0;
const MIN_FPS: u16 = 1;
const MAX_FPS: u16 = 60;
const MIN_FRAMES: u32 = 1;
const MAX_FRAMES: u32 = 18_000;
const MAX_TEXT_CHARS: usize = 200;
const MAX_FIGLET_FONT_CHARS: usize = 64;
const MAX_LAYER_FRAMES: usize = 1000;
const MIN_SCALE: f32 = 0.1;
const MAX_SCALE: f32 = 10.0;
const MIN_OPACITY: f32 = 0.0;
const MAX_OPACITY: f32 = 1.0;
const MIN_OFFSET: i32 = -10_000;
const MAX_OFFSET: i32 = 10_000;
const MIN_OUTPUT_DIMENSION: u32 = 16;
const MAX_OUTPUT_DIMENSION: u32 = 8192;

const MIN_CHARS_PER_FRAME: f32 = 0.01;
const MAX_CHARS_PER_FRAME: f32 = 100.0;
const MIN_INTENSITY: f32 = 0.0;
const MAX_INTENSITY: f32 = 1.0;
const MIN_SPEED: f32 = 0.0;
const MAX_SPEED: f32 = 20.0;
const MIN_AMPLITUDE: f32 = 0.0;
const MAX_AMPLITUDE: f32 = 100.0;
const MIN_FREQUENCY: f32 = 0.0;
const MAX_FREQUENCY: f32 = 50.0;
const MIN_BLINK_PERIOD: u32 = 1;
const MAX_BLINK_PERIOD: u32 = 18_000;

/// A single scene invariant violation.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum ValidationError {
    #[error("unsupported scene version {found}; expected {expected}")]
    UnsupportedVersion { found: u32, expected: u32 },

    #[error("canvas cols {value} out of range {min}..={max}")]
    CanvasColsOutOfRange { value: u16, min: u16, max: u16 },

    #[error("canvas rows {value} out of range {min}..={max}")]
    CanvasRowsOutOfRange { value: u16, min: u16, max: u16 },

    #[error("canvas cell_aspect {value} out of range {min}..={max}")]
    CellAspectOutOfRange { value: f32, min: f32, max: f32 },

    #[error("timeline fps {value} out of range {min}..={max}")]
    FpsOutOfRange { value: u16, min: u16, max: u16 },

    #[error("timeline frames {value} out of range {min}..={max}")]
    FramesOutOfRange { value: u32, min: u32, max: u32 },

    #[error("duplicate layer id \"{id}\"")]
    DuplicateLayerId { id: String },

    #[error("layer \"{layer}\" text must not be empty")]
    EmptyLayerText { layer: String },

    #[error("layer \"{layer}\" text has {len} characters; maximum is {max}")]
    LayerTextTooLong {
        layer: String,
        len: usize,
        max: usize,
    },

    #[error("layer \"{layer}\" figlet font must not be empty")]
    EmptyFigletFont { layer: String },

    #[error("layer \"{layer}\" figlet font has {len} characters; maximum is {max}")]
    FigletFontTooLong {
        layer: String,
        len: usize,
        max: usize,
    },

    #[error("layer \"{layer}\" frames must not be empty")]
    EmptyFrames { layer: String },

    #[error("layer \"{layer}\" has {count} frames; maximum is {max}")]
    FramesTooMany {
        layer: String,
        count: usize,
        max: usize,
    },

    #[error("layer \"{layer}\" frame {frame} has zero dimensions: {cols}x{rows}")]
    FrameGridEmpty {
        layer: String,
        frame: usize,
        cols: u16,
        rows: u16,
    },

    #[error(
        "layer \"{layer}\" frame {frame} has {cells} cells; expected {expected} for {cols}x{rows}"
    )]
    FrameGridCellCount {
        layer: String,
        frame: usize,
        cols: u16,
        rows: u16,
        cells: usize,
        expected: usize,
    },

    #[error("{location} opacity {value} out of range {min}..={max}")]
    TransformOpacityOutOfRange {
        location: String,
        value: f32,
        min: f32,
        max: f32,
    },

    #[error("{location} scale {value} out of range {min}..={max}")]
    TransformScaleOutOfRange {
        location: String,
        value: f32,
        min: f32,
        max: f32,
    },

    #[error("{location} x offset {value} out of range {min}..={max}")]
    TransformXOutOfRange {
        location: String,
        value: i32,
        min: i32,
        max: i32,
    },

    #[error("{location} y offset {value} out of range {min}..={max}")]
    TransformYOutOfRange {
        location: String,
        value: i32,
        min: i32,
        max: i32,
    },

    #[error(
        "layer \"{layer}\" keyframes must be strictly increasing; frame {frame} follows {previous}"
    )]
    KeyframesOutOfOrder {
        layer: String,
        previous: u32,
        frame: u32,
    },

    #[error("layer \"{layer}\" keyframe {frame} must be before timeline end {frames}")]
    KeyframeOutOfRange {
        layer: String,
        frame: u32,
        frames: u32,
    },

    #[error(
        "layer \"{layer}\" keyframe {frame} has invalid cubic-bezier control points ({x1}, {y1}, {x2}, {y2}); x1 and x2 must be finite and in 0..=1"
    )]
    InvalidCubicBezier {
        layer: String,
        frame: u32,
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
    },

    #[error("duplicate effect id \"{id}\"")]
    DuplicateEffectId { id: String },

    #[error("effect \"{id}\" start {start} must be before end {end}")]
    EffectRangeInvalid { id: String, start: u32, end: u32 },

    #[error("effect \"{id}\" end {end} exceeds timeline frames {frames}")]
    EffectEndOutOfRange { id: String, end: u32, frames: u32 },

    #[error("effect \"{id}\" targets unknown layer \"{target}\"")]
    EffectTargetNotFound { id: String, target: String },

    #[error("effect \"{id}\" parameter {parameter} value {value} out of range {min}..={max}")]
    EffectParameterOutOfRange {
        id: String,
        parameter: &'static str,
        value: f32,
        min: f32,
        max: f32,
    },

    #[error("effect \"{id}\" parameter {parameter} value {value} out of range {min}..={max}")]
    EffectOffsetOutOfRange {
        id: String,
        parameter: &'static str,
        value: i32,
        min: i32,
        max: i32,
    },

    #[error("effect \"{id}\" period {value} out of range {min}..={max}")]
    EffectPeriodOutOfRange {
        id: String,
        value: u32,
        min: u32,
        max: u32,
    },

    #[error("output width {value} out of range {min}..={max}")]
    OutputWidthOutOfRange { value: u32, min: u32, max: u32 },

    #[error("output height {value} out of range {min}..={max}")]
    OutputHeightOutOfRange { value: u32, min: u32, max: u32 },

    #[error("output format {format} requires even width and height, got {width}x{height}")]
    OddVideoDimensions {
        format: &'static str,
        width: u32,
        height: u32,
    },
}

/// Validate every invariant of `scene`, returning all violations.
///
/// Errors are reported in document order: version, canvas, timeline, layers,
/// effects, output.
pub fn validate(scene: &Scene) -> Result<(), Vec<ValidationError>> {
    let mut errors = Vec::new();

    validate_version(scene, &mut errors);
    validate_canvas(&scene.canvas, &mut errors);
    validate_timeline(&scene.timeline, &mut errors);
    validate_layers(&scene.layers, &scene.timeline, &mut errors);
    validate_effects(&scene.effects, &scene.layers, &scene.timeline, &mut errors);
    validate_output(&scene.output, &mut errors);

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn validate_version(scene: &Scene, errors: &mut Vec<ValidationError>) {
    if scene.version != SCENE_VERSION {
        errors.push(ValidationError::UnsupportedVersion {
            found: scene.version,
            expected: SCENE_VERSION,
        });
    }
}

fn validate_canvas(canvas: &Canvas, errors: &mut Vec<ValidationError>) {
    if !(MIN_CANVAS_DIMENSION..=MAX_CANVAS_DIMENSION).contains(&canvas.cols) {
        errors.push(ValidationError::CanvasColsOutOfRange {
            value: canvas.cols,
            min: MIN_CANVAS_DIMENSION,
            max: MAX_CANVAS_DIMENSION,
        });
    }
    if !(MIN_CANVAS_DIMENSION..=MAX_CANVAS_DIMENSION).contains(&canvas.rows) {
        errors.push(ValidationError::CanvasRowsOutOfRange {
            value: canvas.rows,
            min: MIN_CANVAS_DIMENSION,
            max: MAX_CANVAS_DIMENSION,
        });
    }
    if !(MIN_CELL_ASPECT..=MAX_CELL_ASPECT).contains(&canvas.cell_aspect) {
        errors.push(ValidationError::CellAspectOutOfRange {
            value: canvas.cell_aspect,
            min: MIN_CELL_ASPECT,
            max: MAX_CELL_ASPECT,
        });
    }
}

fn validate_timeline(timeline: &Timeline, errors: &mut Vec<ValidationError>) {
    if !(MIN_FPS..=MAX_FPS).contains(&timeline.fps) {
        errors.push(ValidationError::FpsOutOfRange {
            value: timeline.fps,
            min: MIN_FPS,
            max: MAX_FPS,
        });
    }
    if !(MIN_FRAMES..=MAX_FRAMES).contains(&timeline.frames) {
        errors.push(ValidationError::FramesOutOfRange {
            value: timeline.frames,
            min: MIN_FRAMES,
            max: MAX_FRAMES,
        });
    }
}

fn validate_layers(layers: &[Layer], timeline: &Timeline, errors: &mut Vec<ValidationError>) {
    let mut seen = HashSet::new();
    let mut duplicates = HashSet::new();

    for layer in layers {
        if !seen.insert(layer.id.as_str()) && duplicates.insert(layer.id.as_str()) {
            errors.push(ValidationError::DuplicateLayerId {
                id: layer.id.clone(),
            });
        }
        validate_layer_kind(layer, errors);
        validate_transform(&layer.transform, &layer_location(layer), errors);
        validate_keyframes(layer, timeline, errors);
    }
}

fn validate_layer_kind(layer: &Layer, errors: &mut Vec<ValidationError>) {
    match &layer.kind {
        LayerKind::Text(text) => validate_layer_text(&layer.id, &text.text, errors),
        LayerKind::Figlet(figlet) => {
            validate_layer_text(&layer.id, &figlet.text, errors);
            validate_figlet_font(&layer.id, &figlet.font, errors);
        }
        LayerKind::Grid(_) => {}
        LayerKind::Frames(frames) => validate_frames(&layer.id, &frames.frames, errors),
    }
}

fn validate_frames(layer: &str, frames: &[Grid], errors: &mut Vec<ValidationError>) {
    if frames.is_empty() {
        errors.push(ValidationError::EmptyFrames {
            layer: layer.to_string(),
        });
        return;
    }
    if frames.len() > MAX_LAYER_FRAMES {
        errors.push(ValidationError::FramesTooMany {
            layer: layer.to_string(),
            count: frames.len(),
            max: MAX_LAYER_FRAMES,
        });
    }
    for (frame, grid) in frames.iter().enumerate() {
        validate_frame_grid(layer, frame, grid, errors);
    }
}

fn validate_frame_grid(layer: &str, frame: usize, grid: &Grid, errors: &mut Vec<ValidationError>) {
    if grid.cols == 0 || grid.rows == 0 {
        errors.push(ValidationError::FrameGridEmpty {
            layer: layer.to_string(),
            frame,
            cols: grid.cols,
            rows: grid.rows,
        });
        return;
    }
    let expected = usize::from(grid.cols) * usize::from(grid.rows);
    if grid.cells.len() != expected {
        errors.push(ValidationError::FrameGridCellCount {
            layer: layer.to_string(),
            frame,
            cols: grid.cols,
            rows: grid.rows,
            cells: grid.cells.len(),
            expected,
        });
    }
}

fn validate_layer_text(layer: &str, text: &str, errors: &mut Vec<ValidationError>) {
    if text.is_empty() {
        errors.push(ValidationError::EmptyLayerText {
            layer: layer.to_string(),
        });
        return;
    }
    let len = text.chars().count();
    if len > MAX_TEXT_CHARS {
        errors.push(ValidationError::LayerTextTooLong {
            layer: layer.to_string(),
            len,
            max: MAX_TEXT_CHARS,
        });
    }
}

fn validate_figlet_font(layer: &str, font: &str, errors: &mut Vec<ValidationError>) {
    if font.is_empty() {
        errors.push(ValidationError::EmptyFigletFont {
            layer: layer.to_string(),
        });
        return;
    }
    let len = font.chars().count();
    if len > MAX_FIGLET_FONT_CHARS {
        errors.push(ValidationError::FigletFontTooLong {
            layer: layer.to_string(),
            len,
            max: MAX_FIGLET_FONT_CHARS,
        });
    }
}

fn layer_location(layer: &Layer) -> String {
    format!("layer \"{}\" transform", layer.id)
}

fn keyframe_location(layer: &Layer, frame: u32) -> String {
    format!("layer \"{}\" keyframe {frame} transform", layer.id)
}

fn validate_transform(transform: &Transform, location: &str, errors: &mut Vec<ValidationError>) {
    if !(MIN_OPACITY..=MAX_OPACITY).contains(&transform.opacity) {
        errors.push(ValidationError::TransformOpacityOutOfRange {
            location: location.to_string(),
            value: transform.opacity,
            min: MIN_OPACITY,
            max: MAX_OPACITY,
        });
    }
    if !(MIN_SCALE..=MAX_SCALE).contains(&transform.scale) {
        errors.push(ValidationError::TransformScaleOutOfRange {
            location: location.to_string(),
            value: transform.scale,
            min: MIN_SCALE,
            max: MAX_SCALE,
        });
    }
    if !(MIN_OFFSET..=MAX_OFFSET).contains(&transform.x) {
        errors.push(ValidationError::TransformXOutOfRange {
            location: location.to_string(),
            value: transform.x,
            min: MIN_OFFSET,
            max: MAX_OFFSET,
        });
    }
    if !(MIN_OFFSET..=MAX_OFFSET).contains(&transform.y) {
        errors.push(ValidationError::TransformYOutOfRange {
            location: location.to_string(),
            value: transform.y,
            min: MIN_OFFSET,
            max: MAX_OFFSET,
        });
    }
}

fn validate_keyframes(layer: &Layer, timeline: &Timeline, errors: &mut Vec<ValidationError>) {
    let mut previous: Option<u32> = None;

    for keyframe in &layer.keyframes {
        if let Some(previous_frame) = previous
            && keyframe.frame <= previous_frame
        {
            errors.push(ValidationError::KeyframesOutOfOrder {
                layer: layer.id.clone(),
                previous: previous_frame,
                frame: keyframe.frame,
            });
        }
        previous = Some(keyframe.frame);

        if keyframe.frame >= timeline.frames {
            errors.push(ValidationError::KeyframeOutOfRange {
                layer: layer.id.clone(),
                frame: keyframe.frame,
                frames: timeline.frames,
            });
        }
        validate_transform(
            &keyframe.transform,
            &keyframe_location(layer, keyframe.frame),
            errors,
        );
        validate_easing(layer, keyframe.frame, keyframe.easing, errors);
    }
}

fn validate_easing(layer: &Layer, frame: u32, easing: Easing, errors: &mut Vec<ValidationError>) {
    if let Easing::CubicBezier { x1, y1, x2, y2 } = easing
        && (![x1, y1, x2, y2].iter().all(|value| value.is_finite())
            || !(0.0..=1.0).contains(&x1)
            || !(0.0..=1.0).contains(&x2))
    {
        errors.push(ValidationError::InvalidCubicBezier {
            layer: layer.id.clone(),
            frame,
            x1,
            y1,
            x2,
            y2,
        });
    }
}

fn validate_effects(
    effects: &[Effect],
    layers: &[Layer],
    timeline: &Timeline,
    errors: &mut Vec<ValidationError>,
) {
    let layer_ids: HashSet<&str> = layers.iter().map(|layer| layer.id.as_str()).collect();
    let mut seen = HashSet::new();
    let mut duplicates = HashSet::new();

    for effect in effects {
        if !seen.insert(effect.id.as_str()) && duplicates.insert(effect.id.as_str()) {
            errors.push(ValidationError::DuplicateEffectId {
                id: effect.id.clone(),
            });
        }
        if effect.start >= effect.end {
            errors.push(ValidationError::EffectRangeInvalid {
                id: effect.id.clone(),
                start: effect.start,
                end: effect.end,
            });
        }
        if effect.end > timeline.frames {
            errors.push(ValidationError::EffectEndOutOfRange {
                id: effect.id.clone(),
                end: effect.end,
                frames: timeline.frames,
            });
        }
        if let Some(target) = &effect.target
            && !layer_ids.contains(target.as_str())
        {
            errors.push(ValidationError::EffectTargetNotFound {
                id: effect.id.clone(),
                target: target.clone(),
            });
        }
        validate_effect_kind(effect, errors);
    }
}

fn validate_effect_kind(effect: &Effect, errors: &mut Vec<ValidationError>) {
    match &effect.kind {
        EffectKind::Typewriter { chars_per_frame } => validate_effect_float(
            &effect.id,
            "chars_per_frame",
            *chars_per_frame,
            MIN_CHARS_PER_FRAME,
            MAX_CHARS_PER_FRAME,
            errors,
        ),
        EffectKind::Fade { from, to } => {
            validate_effect_float(
                &effect.id,
                "from",
                *from,
                MIN_INTENSITY,
                MAX_INTENSITY,
                errors,
            );
            validate_effect_float(&effect.id, "to", *to, MIN_INTENSITY, MAX_INTENSITY, errors);
        }
        EffectKind::Slide { dx, dy } => {
            validate_effect_offset(&effect.id, "dx", *dx, errors);
            validate_effect_offset(&effect.id, "dy", *dy, errors);
        }
        EffectKind::Scanline { intensity, speed } => {
            validate_effect_float(
                &effect.id,
                "intensity",
                *intensity,
                MIN_INTENSITY,
                MAX_INTENSITY,
                errors,
            );
            validate_effect_float(&effect.id, "speed", *speed, MIN_SPEED, MAX_SPEED, errors);
        }
        EffectKind::Glitch { intensity, .. } => validate_effect_float(
            &effect.id,
            "intensity",
            *intensity,
            MIN_INTENSITY,
            MAX_INTENSITY,
            errors,
        ),
        EffectKind::Wave {
            amplitude,
            frequency,
            speed,
        } => {
            validate_effect_float(
                &effect.id,
                "amplitude",
                *amplitude,
                MIN_AMPLITUDE,
                MAX_AMPLITUDE,
                errors,
            );
            validate_effect_float(
                &effect.id,
                "frequency",
                *frequency,
                MIN_FREQUENCY,
                MAX_FREQUENCY,
                errors,
            );
            validate_effect_float(&effect.id, "speed", *speed, MIN_SPEED, MAX_SPEED, errors);
        }
        EffectKind::Dissolve { .. } => {}
        EffectKind::Blink { period } => {
            if !(MIN_BLINK_PERIOD..=MAX_BLINK_PERIOD).contains(period) {
                errors.push(ValidationError::EffectPeriodOutOfRange {
                    id: effect.id.clone(),
                    value: *period,
                    min: MIN_BLINK_PERIOD,
                    max: MAX_BLINK_PERIOD,
                });
            }
        }
    }
}

fn validate_effect_float(
    id: &str,
    parameter: &'static str,
    value: f32,
    min: f32,
    max: f32,
    errors: &mut Vec<ValidationError>,
) {
    if !(min..=max).contains(&value) {
        errors.push(ValidationError::EffectParameterOutOfRange {
            id: id.to_string(),
            parameter,
            value,
            min,
            max,
        });
    }
}

fn validate_effect_offset(
    id: &str,
    parameter: &'static str,
    value: i32,
    errors: &mut Vec<ValidationError>,
) {
    if !(MIN_OFFSET..=MAX_OFFSET).contains(&value) {
        errors.push(ValidationError::EffectOffsetOutOfRange {
            id: id.to_string(),
            parameter,
            value,
            min: MIN_OFFSET,
            max: MAX_OFFSET,
        });
    }
}

fn validate_output(output: &OutputSpec, errors: &mut Vec<ValidationError>) {
    if !(MIN_OUTPUT_DIMENSION..=MAX_OUTPUT_DIMENSION).contains(&output.width) {
        errors.push(ValidationError::OutputWidthOutOfRange {
            value: output.width,
            min: MIN_OUTPUT_DIMENSION,
            max: MAX_OUTPUT_DIMENSION,
        });
    }
    if !(MIN_OUTPUT_DIMENSION..=MAX_OUTPUT_DIMENSION).contains(&output.height) {
        errors.push(ValidationError::OutputHeightOutOfRange {
            value: output.height,
            min: MIN_OUTPUT_DIMENSION,
            max: MAX_OUTPUT_DIMENSION,
        });
    }

    let requires_even_dimensions = matches!(output.format, OutputFormat::Mp4 | OutputFormat::WebM);
    let has_odd_dimension = !output.width.is_multiple_of(2) || !output.height.is_multiple_of(2);
    if requires_even_dimensions && has_odd_dimension {
        errors.push(ValidationError::OddVideoDimensions {
            format: output_format_name(output.format),
            width: output.width,
            height: output.height,
        });
    }
}

fn output_format_name(format: OutputFormat) -> &'static str {
    match format {
        OutputFormat::Png => "png",
        OutputFormat::Jpeg => "jpeg",
        OutputFormat::Svg => "svg",
        OutputFormat::Gif => "gif",
        OutputFormat::Mp4 => "mp4",
        OutputFormat::WebM => "webm",
        OutputFormat::Ansi => "ansi",
        OutputFormat::Text => "text",
    }
}
