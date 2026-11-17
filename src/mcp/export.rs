//! `export` implementation for static and animated artifacts.

use std::fs::{self, File};
use std::io::BufWriter;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::cli::Settings;
use crate::encode::gif::{GifEncoder, GifOptions};
use crate::encode::video::{VideoEncoder, VideoError, VideoFormat, VideoOptions};
use crate::render::{RenderError, RenderOptions, raster, svg};
use crate::scene::compose::compose_frame;
use crate::scene::{Frame, Scene};

use super::error::ToolError;
use super::params::{ExportFormat, ExportParams};
use super::paths;

const DEFAULT_GIF_COLORS: u16 = 256;
const DEFAULT_VIDEO_PRESET: &str = "medium";
const DEFAULT_FFMPEG: &str = "ffmpeg";

/// Validate the scene, write the requested artifact and report its metadata.
pub fn export_scene(params: ExportParams) -> Result<Value, ToolError> {
    let ExportParams {
        scene_path,
        format,
        out,
        frame,
        gif_colors,
        crf,
        preset,
        overwrite,
    } = params;

    if let Some(colors) = gif_colors
        && !(2..=256).contains(&colors)
    {
        return Err(ToolError::invalid_input(format!(
            "gif_colors must be between 2 and 256, got {colors}"
        )));
    }

    let cwd = paths::current_dir()?;
    let (scene_file, scene) = paths::load_scene(&cwd, &scene_path)?;
    let settings = Settings::load()
        .map_err(|error| ToolError::config(format!("could not load settings: {error:#}")))?;

    let relative = match out {
        Some(raw) => {
            let relative = paths::safe_relative(&raw, "out")?;
            paths::require_artifacts_prefix(&relative)?;
            relative
        }
        None => paths::default_export_relative(&scene_file, format.extension()),
    };
    let absolute = paths::prepare_output_path(&cwd, &relative)?;
    paths::ensure_writable_target(&absolute, &relative, overwrite)?;
    paths::ensure_parent(&absolute)?;

    let render_options = render_options(&scene, &settings);
    let frames = if format.is_animated() {
        scene.timeline.frames
    } else {
        1
    };

    let staged = paths::staging_path(&absolute);
    let rendered = match format {
        ExportFormat::Png => {
            let composed = compose(&scene, frame)?;
            raster::render_png(&composed, &render_options, &staged).map_err(render_error)
        }
        ExportFormat::Jpeg => {
            let composed = compose(&scene, frame)?;
            raster::render_jpeg(&composed, &render_options, &staged).map_err(render_error)
        }
        ExportFormat::Svg => {
            let composed = compose(&scene, frame)?;
            let document = svg::render_svg(&composed, &render_options).map_err(render_error)?;
            fs::write(&staged, document).map_err(|error| write_error(&absolute, error))
        }
        ExportFormat::Ansi => {
            let composed = compose(&scene, frame)?;
            fs::write(&staged, raster::frame_to_ansi(&composed, true))
                .map_err(|error| write_error(&absolute, error))
        }
        ExportFormat::Text => {
            let composed = compose(&scene, frame)?;
            fs::write(&staged, raster::frame_to_text(&composed))
                .map_err(|error| write_error(&absolute, error))
        }
        ExportFormat::Gif => export_gif(&scene, gif_colors, &render_options, &staged),
        ExportFormat::Mp4 | ExportFormat::WebM => {
            let ffmpeg = settings
                .config
                .ffmpeg
                .path
                .clone()
                .unwrap_or_else(|| PathBuf::from(DEFAULT_FFMPEG));
            export_video(
                &scene,
                format,
                crf,
                preset,
                &ffmpeg,
                &render_options,
                &staged,
            )
        }
    };
    if let Err(error) = rendered {
        let _ = fs::remove_file(&staged);
        return Err(error);
    }
    paths::finalize_atomic(&staged, &absolute)?;

    let bytes = fs::metadata(&absolute)
        .map_err(|error| write_error(&absolute, error))?
        .len();

    Ok(json!({
        "path": paths::relative_display(&cwd, &absolute)?,
        "format": format.as_str(),
        "mime_type": format.mime_type(),
        "frames": frames,
        "bytes": bytes,
    }))
}

fn compose(scene: &Scene, frame: Option<u32>) -> Result<Frame, ToolError> {
    let frame = frame.unwrap_or(0);
    if frame >= scene.timeline.frames {
        return Err(ToolError::invalid_input(format!(
            "frame {frame} is outside the timeline 0..{}",
            scene.timeline.frames
        )));
    }
    compose_frame(scene, frame)
        .map_err(|error| ToolError::render(format!("could not compose frame {frame}: {error}")))
}

fn export_gif(
    scene: &Scene,
    gif_colors: Option<u16>,
    render_options: &RenderOptions,
    path: &Path,
) -> Result<(), ToolError> {
    let width = u16::try_from(scene.output.width).map_err(|_| {
        ToolError::invalid_input(format!(
            "GIF width {} does not fit in a u16",
            scene.output.width
        ))
    })?;
    let height = u16::try_from(scene.output.height).map_err(|_| {
        ToolError::invalid_input(format!(
            "GIF height {} does not fit in a u16",
            scene.output.height
        ))
    })?;
    let options = GifOptions {
        fps: scene.timeline.fps,
        loop_: scene.timeline.loop_,
        palette_size: gif_colors.unwrap_or(DEFAULT_GIF_COLORS),
        transparent: false,
    };

    let file = File::create(path).map_err(|error| {
        ToolError::export(format!("could not create {}: {error}", path.display()))
    })?;
    let mut encoder = GifEncoder::new(BufWriter::new(file), width, height, options)
        .map_err(|error| ToolError::export(format!("could not start the GIF encoder: {error}")))?;

    for index in 0..scene.timeline.frames {
        let frame = compose_frame(scene, index).map_err(|error| {
            ToolError::render(format!("could not compose frame {index}: {error}"))
        })?;
        let image = raster::frame_to_rgba(&frame, render_options).map_err(render_error)?;
        encoder.write_frame(&image).map_err(|error| {
            ToolError::export(format!("could not encode GIF frame {index}: {error}"))
        })?;
    }
    encoder
        .finish()
        .map_err(|error| ToolError::export(format!("could not finish the GIF: {error}")))
}

fn export_video(
    scene: &Scene,
    format: ExportFormat,
    crf: Option<u8>,
    preset: Option<String>,
    ffmpeg_path: &Path,
    render_options: &RenderOptions,
    path: &Path,
) -> Result<(), ToolError> {
    let video_format = match format {
        ExportFormat::Mp4 => VideoFormat::Mp4,
        ExportFormat::WebM => VideoFormat::WebM,
        _ => {
            return Err(ToolError::invalid_input(
                "video export requires `mp4` or `webm`",
            ));
        }
    };
    let options = VideoOptions {
        width: scene.output.width,
        height: scene.output.height,
        fps: scene.timeline.fps,
        crf: crf.unwrap_or_else(|| video_format.default_crf()),
        preset: preset.unwrap_or_else(|| DEFAULT_VIDEO_PRESET.to_string()),
        ffmpeg_path: ffmpeg_path.to_path_buf(),
    };

    let mut encoder = VideoEncoder::new(path, video_format, &options).map_err(video_error)?;
    for index in 0..scene.timeline.frames {
        let frame = compose_frame(scene, index).map_err(|error| {
            ToolError::render(format!("could not compose frame {index}: {error}"))
        })?;
        let image = raster::frame_to_rgba(&frame, render_options).map_err(render_error)?;
        encoder.write_frame(&image).map_err(video_error)?;
    }
    encoder.finish().map_err(video_error)
}

fn render_options(scene: &Scene, settings: &Settings) -> RenderOptions {
    RenderOptions {
        font_path: settings
            .config
            .render
            .font_path
            .as_ref()
            .map(|path| path.display().to_string()),
        font_size: settings.config.render.font_size,
        scale: 1.0,
        background: Some(scene.canvas.background),
        foreground: None,
        transparent: false,
        target_width: Some(scene.output.width),
        target_height: Some(scene.output.height),
    }
}

fn render_error(error: RenderError) -> ToolError {
    ToolError::render(error.to_string())
}

fn video_error(error: VideoError) -> ToolError {
    let code: &'static str = match error {
        VideoError::Spawn { .. } => "ffmpeg_unavailable",
        VideoError::InvalidOptions { .. } => "invalid_input",
        VideoError::Dimensions { .. } | VideoError::Io { .. } | VideoError::Ffmpeg { .. } => {
            "export_failed"
        }
    };
    ToolError::new(code, error.to_string())
}

fn write_error(path: &Path, error: std::io::Error) -> ToolError {
    ToolError::io(format!("could not write {}: {error}", path.display()))
}
