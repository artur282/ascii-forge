//! Command line interface: argument parsing, configuration and diagnostics.

mod capabilities;

use std::env;
use std::fs;
use std::fs::File;
use std::io::{BufWriter, IsTerminal};
use std::path::{Path, PathBuf};
use std::process::{self, Command as ProcessCommand};

use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand, ValueEnum};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use tracing_subscriber::EnvFilter;

use crate::ai::provider::{OpenAiProvider, api_key_from_env};
use crate::ai::{GeneratedPlan, generate_plan, generation_cache_key, scene_plan_schema};
use crate::cache::ProjectCache;
use crate::encode::audio::{AudioOptions, generate_voice, mux_audio};
use crate::encode::gif::{GifEncoder, GifOptions};
use crate::encode::video::{VideoEncoder, VideoFormat, VideoOptions};
use crate::figlet::font_names;
use crate::import::video::{default_ffprobe, import_video};
use crate::import::{
    ImportOptions, build_static_scene, build_video_scene, image_to_grid, resolve_charset,
};
use crate::render::{RenderOptions, raster, svg};
use crate::scene::compose::compose_frame;
use crate::scene::{Frame, OutputFormat, Rgba, Scene};
use crate::templates::{TemplateParams, build as build_template};
use crate::tui;

/// Well-known monospace font locations, checked in order.
const MONOSPACE_FONT_CANDIDATES: &[&str] = &[
    "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
    "/usr/share/fonts/dejavu/DejaVuSansMono.ttf",
    "/usr/share/fonts/TTF/DejaVuSansMono.ttf",
    "/usr/share/fonts/truetype/liberation/LiberationMono-Regular.ttf",
    "/usr/share/fonts/liberation/LiberationMono-Regular.ttf",
    "/usr/share/fonts/truetype/ubuntu/UbuntuMono-R.ttf",
    "/usr/share/fonts/truetype/noto/NotoSansMono-Regular.ttf",
    "/System/Library/Fonts/Menlo.ttc",
    "/System/Library/Fonts/Supplemental/Menlo.ttc",
    r"C:\Windows\Fonts\consola.ttf",
];

/// ASCII Forge command line interface.
#[derive(Debug, Parser)]
#[command(
    name = "ascii-forge",
    version,
    about = "AI-driven ASCII art, images and videos from the command line"
)]
pub struct Cli {
    /// Enable debug logging on stderr.
    #[arg(long, global = true)]
    pub verbose: bool,

    /// Emit machine-readable JSON where the command supports it.
    #[arg(long, global = true)]
    pub json: bool,

    /// Disable ANSI colors in human output.
    #[arg(long, global = true)]
    pub no_color: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Generate a scene from a text prompt or an offline template.
    Generate(GenerateArgs),
    /// Generate and render a static image in one step from an offline template.
    Make(MakeArgs),
    /// Render a scene to an image, text, animated GIF or video.
    Render(RenderArgs),
    /// Preview a scene as animated ANSI art in the terminal.
    Preview(PreviewArgs),
    /// Import an image or video/GIF as an ASCII scene.
    Import(ImportArgs),
    /// Clear the project-local response cache.
    Cache(CacheArgs),
    /// List the FIGlet fonts and the resolved render font.
    Fonts,
    /// Check FFmpeg, its encoders, monospace fonts and the config directory.
    Doctor,
    /// Print the capability catalog for agents.
    Capabilities,
    /// Start the Model Context Protocol server over stdio.
    #[cfg(feature = "mcp")]
    Mcp,
}

#[derive(Debug, Args)]
pub struct GenerateArgs {
    /// Prompt describing the scene to generate (required without --template).
    #[arg(value_name = "PROMPT")]
    pub prompt: Option<String>,

    /// Offline template; no network or API key is used.
    #[arg(long, value_enum)]
    pub template: Option<TemplateArg>,

    /// Title used by a template or as a plan hint.
    #[arg(long)]
    pub title: Option<String>,

    /// Tagline used by a template or as a plan hint.
    #[arg(long)]
    pub tagline: Option<String>,

    /// Deterministic plan seed.
    #[arg(long)]
    pub seed: Option<u64>,

    /// AI provider name (default from config, then `openai`).
    #[arg(long)]
    pub provider: Option<String>,

    /// Model name (default from config).
    #[arg(long)]
    pub model: Option<String>,

    /// OpenAI-compatible base URL (default from config).
    #[arg(long)]
    pub base_url: Option<String>,

    /// Opt into one vision review pass; requires --vision-model or config.
    #[arg(long)]
    pub vision: bool,

    /// Vision-capable model name for --vision.
    #[arg(long)]
    pub vision_model: Option<String>,

    /// Bypass the project response cache.
    #[arg(long)]
    pub no_cache: bool,

    /// Where to write the generated scene.
    #[arg(
        short,
        long = "out",
        alias = "output",
        value_name = "FILE",
        default_value = "scene.json"
    )]
    pub output: PathBuf,
}

#[derive(Debug, Args)]
pub struct MakeArgs {
    /// Title rendered by the template.
    #[arg(value_name = "TITLE")]
    pub title: String,

    /// Offline template; no network or API key is used.
    #[arg(long, value_enum, default_value = "logo")]
    pub template: TemplateArg,

    /// Tagline rendered below the title.
    #[arg(long)]
    pub tagline: Option<String>,

    /// Deterministic plan seed.
    #[arg(long, default_value_t = 0)]
    pub seed: u64,

    /// Frame to render; defaults to the settled last frame.
    #[arg(long)]
    pub frame: Option<u32>,

    /// Where to write the image; defaults to a slug of the title.
    #[arg(short, long = "out", alias = "output", value_name = "FILE")]
    pub output: Option<PathBuf>,

    /// Static output format (png, jpeg or svg).
    #[arg(long, default_value = "png", value_parser = parse_static_format)]
    pub format: OutputFormat,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum TemplateArg {
    Logo,
    Promo,
    Glitch,
    Matrix,
    #[value(alias = "lower_third")]
    LowerThird,
}

impl From<TemplateArg> for crate::ai::TemplateKind {
    fn from(value: TemplateArg) -> Self {
        match value {
            TemplateArg::Logo => Self::Logo,
            TemplateArg::Promo => Self::Promo,
            TemplateArg::Glitch => Self::Glitch,
            TemplateArg::Matrix => Self::Matrix,
            TemplateArg::LowerThird => Self::LowerThird,
        }
    }
}

#[derive(Debug, Args)]
pub struct RenderArgs {
    /// Scene file to render.
    #[arg(value_name = "SCENE")]
    pub scene: PathBuf,

    /// Where to write the rendered artifact.
    #[arg(short, long = "out", alias = "output", value_name = "FILE")]
    pub output: Option<PathBuf>,

    /// Output format, overriding the scene's own output spec.
    #[arg(long, value_parser = parse_output_format)]
    pub format: Option<OutputFormat>,

    /// Scale factor applied to raster output.
    #[arg(long, default_value_t = 1.0)]
    pub scale: f32,

    /// Frame index for static formats (PNG, JPEG, SVG, ANSI, text).
    #[arg(long, default_value_t = 0)]
    pub frame: u32,

    /// GIF palette size, from 2 to 256 colors.
    #[arg(long = "gif-colors", default_value_t = 256, value_parser = parse_palette_size)]
    pub gif_colors: u16,

    /// Export GIF pixels with alpha below 128 as transparent.
    #[arg(long = "gif-transparent")]
    pub gif_transparent: bool,

    /// Video quality (CRF); lower is better and larger.
    #[arg(long)]
    pub crf: Option<u8>,

    /// Encoder preset (x264 names; mapped to `-cpu-used` for VP9).
    #[arg(long)]
    pub preset: Option<String>,

    /// Video frame rate override.
    #[arg(long)]
    pub fps: Option<u16>,

    /// Video width override in pixels.
    #[arg(long)]
    pub width: Option<u32>,

    /// Video height override in pixels.
    #[arg(long)]
    pub height: Option<u32>,

    /// Path to the FFmpeg binary.
    #[arg(long)]
    pub ffmpeg_path: Option<PathBuf>,

    /// Mux an existing audio file into the MP4.
    #[arg(long, conflicts_with = "voice")]
    pub audio: Option<PathBuf>,

    /// Synthesize a voice-over with espeak-ng and mux it into the MP4.
    #[arg(long, conflicts_with = "audio")]
    pub voice: Option<String>,
}

#[derive(Debug, Args)]
pub struct PreviewArgs {
    /// Scene file to preview.
    #[arg(value_name = "SCENE")]
    pub scene: PathBuf,

    /// Frame index to preview.
    #[arg(long, default_value_t = 0)]
    pub frame: u32,
}

#[derive(Debug, Args)]
pub struct ImportArgs {
    /// Image or video/GIF file to import.
    #[arg(value_name = "INPUT")]
    pub input: PathBuf,

    /// Where to write the imported scene.
    #[arg(
        short,
        long = "out",
        alias = "output",
        value_name = "FILE",
        default_value = "scene.json"
    )]
    pub output: PathBuf,

    /// Target columns.
    #[arg(long, default_value_t = crate::import::DEFAULT_COLS, value_parser = parse_cols)]
    pub cols: u16,

    /// Target rows; derived from the source aspect when omitted.
    #[arg(long, value_parser = parse_rows)]
    pub rows: Option<u16>,

    /// Charset preset (ascii, blocks, detailed) or a custom luminance ramp.
    #[arg(long, value_name = "CHARS")]
    pub charset: Option<String>,

    /// Sample the source color per cell.
    #[arg(long)]
    pub color: bool,

    /// Use 2x4 pixels per cell and Unicode braille output.
    #[arg(long)]
    pub braille: bool,

    /// Apply Floyd-Steinberg dithering.
    #[arg(long)]
    pub dither: bool,

    /// Apply Sobel edge detection with directional glyphs.
    #[arg(long)]
    pub edges: bool,

    /// Invert the luminance mapping.
    #[arg(long)]
    pub invert: bool,

    /// Canvas background color (#rrggbb or rrggbb).
    #[arg(long, value_parser = parse_color)]
    pub background: Option<Rgba>,

    /// Foreground color for monochrome output (#rrggbb or rrggbb).
    #[arg(long, value_parser = parse_color)]
    pub foreground: Option<Rgba>,

    /// Video/GIF sampling rate.
    #[arg(long, default_value_t = crate::import::DEFAULT_FPS, value_parser = parse_fps)]
    pub fps: u16,

    /// Maximum embedded video/GIF frames.
    #[arg(
        long = "max-frames",
        default_value_t = crate::import::DEFAULT_MAX_FRAMES,
        value_parser = parse_max_frames
    )]
    pub max_frames: u32,

    /// Path to the FFmpeg binary for video/GIF import.
    #[arg(long)]
    pub ffmpeg_path: Option<PathBuf>,

    /// Path to the FFprobe binary; derived from --ffmpeg-path when omitted.
    #[arg(long)]
    pub ffprobe_path: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct CacheArgs {
    #[command(subcommand)]
    pub command: CacheCommand,
}

#[derive(Debug, Subcommand)]
pub enum CacheCommand {
    /// Remove cached responses under `.ascii-forge/cache`.
    Clear,
}

/// Parse the format names accepted by `render --format`.
fn parse_output_format(value: &str) -> std::result::Result<OutputFormat, String> {
    match value {
        "png" => Ok(OutputFormat::Png),
        "jpeg" => Ok(OutputFormat::Jpeg),
        "svg" => Ok(OutputFormat::Svg),
        "gif" => Ok(OutputFormat::Gif),
        "mp4" => Ok(OutputFormat::Mp4),
        "webm" => Ok(OutputFormat::WebM),
        "ansi" => Ok(OutputFormat::Ansi),
        "text" => Ok(OutputFormat::Text),
        other => Err(format!(
            "unknown format `{other}` (expected png, jpeg, svg, gif, mp4, webm, ansi or text)"
        )),
    }
}

/// Parse the static format names accepted by `make --format`.
fn parse_static_format(value: &str) -> std::result::Result<OutputFormat, String> {
    match value {
        "png" => Ok(OutputFormat::Png),
        "jpeg" => Ok(OutputFormat::Jpeg),
        "svg" => Ok(OutputFormat::Svg),
        other => Err(format!(
            "unknown format `{other}` (expected png, jpeg or svg)"
        )),
    }
}

fn parse_palette_size(value: &str) -> std::result::Result<u16, String> {
    let size: u16 = value
        .parse()
        .map_err(|_| format!("GIF palette size must be an integer, got `{value}`"))?;
    if (2..=256).contains(&size) {
        Ok(size)
    } else {
        Err(format!(
            "GIF palette size must be between 2 and 256, got {size}"
        ))
    }
}

fn parse_cols(value: &str) -> std::result::Result<u16, String> {
    parse_u16_range(value, "columns", 1, crate::import::MAX_COLS)
}

fn parse_rows(value: &str) -> std::result::Result<u16, String> {
    parse_u16_range(value, "rows", 1, crate::import::MAX_ROWS)
}

fn parse_fps(value: &str) -> std::result::Result<u16, String> {
    parse_u16_range(value, "fps", 1, 60)
}

fn parse_max_frames(value: &str) -> std::result::Result<u32, String> {
    let frames: u32 = value
        .parse()
        .map_err(|_| format!("max frames must be an integer, got `{value}`"))?;
    if (1..=crate::import::MAX_FRAMES).contains(&frames) {
        Ok(frames)
    } else {
        Err(format!(
            "max frames must be between 1 and {}, got {frames}",
            crate::import::MAX_FRAMES
        ))
    }
}

fn parse_u16_range(
    value: &str,
    name: &str,
    min: u16,
    max: u16,
) -> std::result::Result<u16, String> {
    let number: u16 = value
        .parse()
        .map_err(|_| format!("{name} must be an integer, got `{value}`"))?;
    if (min..=max).contains(&number) {
        Ok(number)
    } else {
        Err(format!(
            "{name} must be between {min} and {max}, got {number}"
        ))
    }
}

fn parse_color(value: &str) -> std::result::Result<Rgba, String> {
    let hex = value.trim().strip_prefix('#').unwrap_or(value.trim());
    if hex.len() != 6 || !hex.is_ascii() {
        return Err(format!("color must be #rrggbb or rrggbb, got `{value}`"));
    }
    let channel = |range: std::ops::Range<usize>| {
        u8::from_str_radix(&hex[range], 16)
            .map_err(|_| format!("color must be #rrggbb or rrggbb, got `{value}`"))
    };
    Ok(Rgba::rgb(channel(0..2)?, channel(2..4)?, channel(4..6)?))
}

pub fn run() -> Result<()> {
    let Cli {
        verbose,
        json,
        no_color,
        command,
    } = Cli::parse();

    if verbose {
        init_tracing();
    }
    tracing::debug!("starting ascii-forge {}", env!("CARGO_PKG_VERSION"));

    match command {
        Command::Generate(args) => run_generate(args, json),
        Command::Make(args) => run_make(args, json),
        Command::Render(args) => run_render(args, json, no_color),
        Command::Preview(args) => run_preview(args, json, no_color),
        Command::Import(args) => run_import(args, json),
        Command::Cache(args) => run_cache(args, json),
        Command::Fonts => run_fonts(json),
        Command::Doctor => run_doctor(json, no_color),
        Command::Capabilities => capabilities::run(json),
        #[cfg(feature = "mcp")]
        Command::Mcp => crate::mcp::run_stdio(),
    }
}

#[derive(Debug, Clone, Copy)]
enum PlanOrigin {
    Ai,
    Template(TemplateArg),
}

impl PlanOrigin {
    fn source(self) -> &'static str {
        match self {
            Self::Ai => "ai",
            Self::Template(_) => "template",
        }
    }

    fn template(self) -> Option<&'static str> {
        match self {
            Self::Ai => None,
            Self::Template(template) => Some(template.as_str()),
        }
    }
}

impl TemplateArg {
    fn as_str(self) -> &'static str {
        match self {
            Self::Logo => "logo",
            Self::Promo => "promo",
            Self::Glitch => "glitch",
            Self::Matrix => "matrix",
            Self::LowerThird => "lower_third",
        }
    }
}

fn run_generate(args: GenerateArgs, json: bool) -> Result<()> {
    let settings = Settings::load()?;

    let (origin, mut generated, api_key) = if let Some(template) = args.template {
        let title = args
            .title
            .clone()
            .filter(|title| !title.trim().is_empty())
            .context("template mode requires --title")?;
        let tagline = args
            .tagline
            .clone()
            .filter(|tagline| !tagline.trim().is_empty());
        let plan = build_template(
            template.into(),
            &TemplateParams {
                title,
                tagline,
                seed: args.seed.unwrap_or(0),
            },
        )?;
        let scene = crate::ai::compile_plan(&plan)?;
        (
            PlanOrigin::Template(template),
            GeneratedPlan {
                plan,
                scene,
                usage: None,
                attempts: 0,
                cached: false,
            },
            None,
        )
    } else {
        let prompt = args
            .prompt
            .clone()
            .filter(|prompt| !prompt.trim().is_empty())
            .context("generate requires a prompt unless --template is used")?;
        let provider_name = args
            .provider
            .clone()
            .or(settings.config.ai.provider.clone())
            .unwrap_or_else(|| "openai".to_string())
            .to_lowercase();
        let model = args
            .model
            .clone()
            .or(settings.config.ai.model.clone())
            .unwrap_or_else(|| "gpt-4o-mini".to_string());
        let base_url = args
            .base_url
            .clone()
            .or(settings.config.ai.base_url.clone())
            .unwrap_or_else(|| "https://api.openai.com/v1".to_string());
        let api_key = api_key_from_env()?;
        let provider = OpenAiProvider::new(api_key.clone(), base_url, model.clone())?;
        let schema = scene_plan_schema();
        let key = generation_cache_key(&provider_name, &model, &prompt, &schema);
        let cache = if args.no_cache {
            None
        } else {
            Some(ProjectCache::discover(
                &env::current_dir().context("could not resolve the current directory")?,
            ))
        };
        let cache_ref = cache.as_ref().map(|cache| (cache, key.as_str()));
        let mut generated = generate_plan(&provider, &prompt, &schema, cache_ref)?;

        if let Some(title) = args.title.clone() {
            generated.plan.title = title;
        }
        if let Some(tagline) = args.tagline.clone() {
            generated.plan.tagline = if tagline.trim().is_empty() {
                None
            } else {
                Some(tagline)
            };
        }
        if let Some(seed) = args.seed {
            generated.plan.seed = seed;
        }
        if args.title.is_some() || args.tagline.is_some() || args.seed.is_some() {
            generated.scene = crate::ai::compile_plan(&generated.plan)?;
        }

        (PlanOrigin::Ai, generated, Some(api_key))
    };

    let mut vision_applied = false;
    if args.vision {
        let vision_model = args
            .vision_model
            .clone()
            .or(settings.config.ai.vision_model.clone())
            .context("--vision requires --vision-model or [ai].vision_model")?;
        let api_key = api_key.context("--vision requires an API key")?;
        let base_url = args
            .base_url
            .clone()
            .or(settings.config.ai.base_url.clone())
            .unwrap_or_else(|| "https://api.openai.com/v1".to_string());
        match apply_vision(
            &mut generated,
            &api_key,
            &base_url,
            &vision_model,
            args.no_cache,
        ) {
            Ok(applied) => vision_applied = applied,
            Err(error) => eprintln!("warning: vision review skipped: {error:#}"),
        }
    }

    write_scene(&args.output, &generated.scene)?;
    report_generation(json, &args.output, origin, &generated, vision_applied)
}

fn apply_vision(
    generated: &mut GeneratedPlan,
    api_key: &str,
    base_url: &str,
    vision_model: &str,
    no_cache: bool,
) -> Result<bool> {
    let provider = crate::ai::vision::OpenAiVisionProvider::new(api_key, base_url, vision_model)?;
    let cache = if no_cache {
        None
    } else {
        Some(ProjectCache::discover(
            &env::current_dir().context("could not resolve the current directory")?,
        ))
    };

    let revised = crate::ai::vision::review_once(
        &generated.plan,
        &generated.scene,
        &provider,
        cache.as_ref(),
    )?;
    let Some(revised) = revised else {
        return Ok(false);
    };

    match crate::ai::compile_plan(&revised) {
        Ok(scene) => {
            generated.plan = revised;
            generated.scene = scene;
            Ok(true)
        }
        Err(error) => {
            eprintln!("warning: vision revision was rejected: {error}");
            Ok(false)
        }
    }
}

fn run_import(args: ImportArgs, json: bool) -> Result<()> {
    let settings = Settings::load()?;
    let options = ImportOptions {
        cols: args.cols,
        rows: args.rows,
        charset: resolve_charset(args.charset.as_deref())?,
        color: args.color,
        braille: args.braille,
        dither: args.dither,
        invert: args.invert,
        edges: args.edges,
        background: args.background.unwrap_or(Rgba::rgb(0, 0, 0)),
        foreground: args.foreground.unwrap_or(Rgba::rgb(255, 255, 255)),
        cell_aspect: crate::import::DEFAULT_CELL_ASPECT,
        fps: args.fps,
        max_frames: args.max_frames,
    };
    options.validate()?;

    let extension = args
        .input
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let video_extension = matches!(
        extension.as_str(),
        "gif" | "mp4" | "webm" | "mov" | "mkv" | "avi" | "m4v" | "mpeg" | "mpg" | "ogv" | "ts"
    );

    let (scene, kind) = if video_extension {
        let ffmpeg = args
            .ffmpeg_path
            .clone()
            .or(settings.config.ffmpeg.path.clone())
            .unwrap_or_else(|| PathBuf::from("ffmpeg"));
        let ffprobe = args
            .ffprobe_path
            .clone()
            .unwrap_or_else(|| default_ffprobe(&ffmpeg));
        let imported = import_video(&args.input, &options, &ffmpeg, &ffprobe)?;
        let fps = imported.fps;
        (build_video_scene(imported.frames, fps, &options)?, "video")
    } else {
        let image = image::open(&args.input)
            .with_context(|| format!("failed to open image {}", args.input.display()))?;
        let grid = image_to_grid(&image, &options)?;
        (build_static_scene(grid, &options)?, "image")
    };

    write_scene(&args.output, &scene)?;
    report_import(json, &args.output, kind, &scene, &options)
}

fn report_import(
    json: bool,
    path: &Path,
    kind: &str,
    scene: &Scene,
    options: &ImportOptions,
) -> Result<()> {
    if json {
        print_json(&serde_json::json!({
            "output": path.display().to_string(),
            "kind": kind,
            "cols": scene.canvas.cols,
            "rows": scene.canvas.rows,
            "frames": scene.timeline.frames,
            "fps": scene.timeline.fps,
            "color": options.color,
            "braille": options.braille,
            "dither": options.dither,
            "edges": options.edges,
        }))?;
    } else {
        println!(
            "wrote {} ({kind} {}x{}, {} frames, {} fps)",
            path.display(),
            scene.canvas.cols,
            scene.canvas.rows,
            scene.timeline.frames,
            scene.timeline.fps
        );
    }
    Ok(())
}

fn run_cache(args: CacheArgs, json: bool) -> Result<()> {
    match args.command {
        CacheCommand::Clear => clear_cache(json),
    }
}

fn clear_cache(json: bool) -> Result<()> {
    let cwd = env::current_dir().context("could not resolve the current directory")?;
    let cache = ProjectCache::discover(&cwd);
    let cleared = cache
        .clear()
        .with_context(|| format!("failed to clear {}", cache.cache_dir().display()))?;

    if json {
        print_json(&serde_json::json!({
            "path": cleared.path().display().to_string(),
            "removed": cleared.removed(),
        }))
    } else {
        let noun = if cleared.removed() == 1 {
            "entry"
        } else {
            "entries"
        };
        println!(
            "removed {} cached {noun} from {}",
            cleared.removed(),
            cleared.path().display()
        );
        Ok(())
    }
}

fn write_scene(path: &Path, scene: &Scene) -> Result<()> {
    ensure_parent(path)?;
    let contents = serde_json::to_string_pretty(scene).context("could not serialize the scene")?;
    fs::write(path, contents).with_context(|| format!("failed to write {}", path.display()))
}

fn report_generation(
    json: bool,
    path: &Path,
    origin: PlanOrigin,
    generated: &GeneratedPlan,
    vision: bool,
) -> Result<()> {
    if let Some(usage) = generated.usage {
        eprintln!(
            "tokens: prompt={} completion={} total={}",
            usage.prompt_tokens, usage.completion_tokens, usage.total_tokens
        );
    }

    if json {
        print_json(&serde_json::json!({
            "output": path.display().to_string(),
            "source": origin.source(),
            "template": origin.template(),
            "frames": generated.scene.timeline.frames,
            "seed": generated.scene.seed,
            "attempts": generated.attempts,
            "cached": generated.cached,
            "usage": generated.usage,
            "vision": vision,
        }))?;
    } else {
        println!(
            "wrote {} ({} frames, seed {})",
            path.display(),
            generated.scene.timeline.frames,
            generated.scene.seed
        );
    }
    Ok(())
}

/// Render an offline template to a static image in one step.
fn run_make(args: MakeArgs, json: bool) -> Result<()> {
    let settings = Settings::load()?;
    let title = args.title.trim().to_string();
    if title.is_empty() {
        bail!("title must not be empty");
    }

    let tagline = args
        .tagline
        .as_deref()
        .map(str::trim)
        .filter(|tagline| !tagline.is_empty())
        .map(str::to_string);
    let plan = build_template(
        args.template.into(),
        &TemplateParams {
            title: title.clone(),
            tagline: tagline.clone(),
            seed: args.seed,
        },
    )?;
    let scene = crate::ai::compile_plan(&plan)?;
    validate_scene(&scene)?;

    let frame_index = args
        .frame
        .unwrap_or_else(|| scene.timeline.frames.saturating_sub(1));
    let frame = compose_frame(&scene, frame_index)?;

    let format = args.format;
    let path = args.output.unwrap_or_else(|| {
        PathBuf::from(format!("{}.{}", slugify(&title), static_extension(format)))
    });
    ensure_parent(&path)?;
    write_static(
        &frame,
        format,
        &render_options(&scene, &settings, 1.0),
        &path,
    )?;

    report_make(
        json,
        &MakeReport {
            path: &path,
            format,
            scene: &scene,
            frame: frame_index,
            template: args.template,
            title: &title,
            tagline: tagline.as_deref(),
            seed: args.seed,
        },
    )
}

fn write_static(
    frame: &Frame,
    format: OutputFormat,
    options: &RenderOptions,
    path: &Path,
) -> Result<()> {
    match format {
        OutputFormat::Png => raster::render_png(frame, options, path)?,
        OutputFormat::Jpeg => raster::render_jpeg(frame, options, path)?,
        OutputFormat::Svg => {
            let document = svg::render_svg(frame, options)?;
            fs::write(path, document)
                .with_context(|| format!("failed to write {}", path.display()))?;
        }
        other => bail!(
            "format `{}` is not a static image format",
            format_name(other)
        ),
    }
    Ok(())
}

/// Filename extension used by `make` for a static format.
fn static_extension(format: OutputFormat) -> &'static str {
    match format {
        OutputFormat::Jpeg => "jpg",
        OutputFormat::Svg => "svg",
        _ => "png",
    }
}

struct MakeReport<'a> {
    path: &'a Path,
    format: OutputFormat,
    scene: &'a Scene,
    frame: u32,
    template: TemplateArg,
    title: &'a str,
    tagline: Option<&'a str>,
    seed: u64,
}

fn report_make(json: bool, report: &MakeReport<'_>) -> Result<()> {
    if json {
        print_json(&serde_json::json!({
            "output": report.path.display().to_string(),
            "format": format_name(report.format),
            "width": report.scene.output.width,
            "height": report.scene.output.height,
            "template": report.template.as_str(),
            "title": report.title,
            "tagline": report.tagline,
            "seed": report.seed,
            "frame": report.frame,
            "frames": report.scene.timeline.frames,
        }))
    } else {
        println!(
            "wrote {} ({}x{}, {}, frame {}/{})",
            report.path.display(),
            report.scene.output.width,
            report.scene.output.height,
            report.template.as_str(),
            report.frame,
            report.scene.timeline.frames
        );
        Ok(())
    }
}

const MAX_SLUG_LEN: usize = 48;

const ACCENT_FOLDS: &[(char, char)] = &[
    ('á', 'a'),
    ('é', 'e'),
    ('í', 'i'),
    ('ó', 'o'),
    ('ú', 'u'),
    ('ü', 'u'),
    ('ñ', 'n'),
    ('ç', 'c'),
];

fn fold_accent(character: char) -> char {
    ACCENT_FOLDS
        .iter()
        .find_map(|(accented, plain)| (*accented == character).then_some(*plain))
        .unwrap_or(character)
}

/// Build a filename-safe slug from a title.
fn slugify(title: &str) -> String {
    let mut slug = String::new();
    let mut separator = false;
    for raw in title.chars() {
        let character = raw.to_lowercase().next().unwrap_or(raw);
        let character = fold_accent(character);
        if character.is_ascii_alphanumeric() {
            if separator && !slug.is_empty() {
                slug.push('-');
            }
            separator = false;
            slug.push(character);
        } else {
            separator = true;
        }
    }

    if slug.len() > MAX_SLUG_LEN {
        slug.truncate(MAX_SLUG_LEN);
        slug = slug.trim_end_matches('-').to_string();
    }
    if slug.is_empty() {
        "image".to_string()
    } else {
        slug
    }
}

fn run_render(args: RenderArgs, json: bool, no_color: bool) -> Result<()> {
    let settings = Settings::load()?;
    let scene = load_scene(&args.scene)?;
    validate_scene(&scene)?;

    let format = args.format.unwrap_or(scene.output.format);
    let options = render_options(&scene, &settings, args.scale);

    match format {
        OutputFormat::Png => {
            let frame = compose_frame(&scene, args.frame)?;
            let path = output_path(&args.scene, args.output.as_deref(), "png");
            ensure_parent(&path)?;
            raster::render_png(&frame, &options, &path)?;
            report_output(json, &path, "png")
        }
        OutputFormat::Jpeg => {
            let frame = compose_frame(&scene, args.frame)?;
            let path = output_path(&args.scene, args.output.as_deref(), "jpg");
            ensure_parent(&path)?;
            raster::render_jpeg(&frame, &options, &path)?;
            report_output(json, &path, "jpeg")
        }
        OutputFormat::Svg => {
            let frame = compose_frame(&scene, args.frame)?;
            let document = svg::render_svg(&frame, &options)?;
            match args.output.as_deref() {
                Some(path) => {
                    ensure_parent(path)?;
                    fs::write(path, document)
                        .with_context(|| format!("failed to write {}", path.display()))?;
                    report_output(json, path, "svg")
                }
                None => {
                    print!("{document}");
                    Ok(())
                }
            }
        }
        OutputFormat::Ansi => {
            let frame = compose_frame(&scene, args.frame)?;
            let text = raster::frame_to_ansi(&frame, color_enabled(no_color));
            write_text_output(args.output.as_deref(), &text, json, "ansi")
        }
        OutputFormat::Text => {
            let frame = compose_frame(&scene, args.frame)?;
            let text = raster::frame_to_text(&frame);
            write_text_output(args.output.as_deref(), &text, json, "text")
        }
        OutputFormat::Gif => run_gif(&scene, &args, &settings, json),
        OutputFormat::Mp4 | OutputFormat::WebM => run_video(&scene, &args, &settings, format, json),
    }
}

fn render_options(scene: &Scene, settings: &Settings, scale: f32) -> RenderOptions {
    RenderOptions {
        font_path: settings
            .config
            .render
            .font_path
            .as_ref()
            .map(|path| path.display().to_string()),
        font_size: settings.config.render.font_size,
        scale,
        background: Some(scene.canvas.background),
        foreground: None,
        transparent: false,
        target_width: Some(scene.output.width),
        target_height: Some(scene.output.height),
    }
}

fn run_gif(scene: &Scene, args: &RenderArgs, settings: &Settings, json: bool) -> Result<()> {
    warn_if_large_gif(scene);

    let path = output_path(&args.scene, args.output.as_deref(), "gif");
    ensure_parent(&path)?;
    let file =
        File::create(&path).with_context(|| format!("failed to create {}", path.display()))?;
    let writer = BufWriter::new(file);
    let width = u16::try_from(scene.output.width).context("GIF width does not fit in u16")?;
    let height = u16::try_from(scene.output.height).context("GIF height does not fit in u16")?;
    let options = GifOptions {
        fps: scene.timeline.fps,
        loop_: scene.timeline.loop_,
        palette_size: args.gif_colors,
        transparent: args.gif_transparent,
    };
    let mut encoder = GifEncoder::new(writer, width, height, options)?;
    let mut render_options = render_options(scene, settings, args.scale);
    render_options.transparent = args.gif_transparent;

    for frame_index in 0..scene.timeline.frames {
        let frame = compose_frame(scene, frame_index)?;
        let image = raster::frame_to_rgba(&frame, &render_options)?;
        encoder.write_frame(&image)?;
    }
    encoder.finish()?;

    report_output(json, &path, "gif")
}

const MIN_VIDEO_DIMENSION: u32 = 16;
const MAX_VIDEO_DIMENSION: u32 = 8192;
const DEFAULT_VIDEO_PRESET: &str = "medium";

fn run_video(
    scene: &Scene,
    args: &RenderArgs,
    settings: &Settings,
    format: OutputFormat,
    json: bool,
) -> Result<()> {
    let video_format = match format {
        OutputFormat::Mp4 => VideoFormat::Mp4,
        OutputFormat::WebM => VideoFormat::WebM,
        _ => unreachable!("run_video only receives MP4 or WebM"),
    };
    let audio_requested = args.audio.is_some() || args.voice.is_some();
    if audio_requested && video_format != VideoFormat::Mp4 {
        bail!("audio muxing is only supported for MP4; use --format mp4");
    }

    let width = video_dimension(args.width.unwrap_or(scene.output.width), "width")?;
    let height = video_dimension(args.height.unwrap_or(scene.output.height), "height")?;
    let fps = args.fps.unwrap_or(scene.timeline.fps);
    let crf = args.crf.unwrap_or_else(|| video_format.default_crf());
    let preset = args
        .preset
        .clone()
        .unwrap_or_else(|| DEFAULT_VIDEO_PRESET.to_string());
    let ffmpeg_path = resolve_ffmpeg_path(settings, args);
    let options = VideoOptions {
        width,
        height,
        fps,
        crf,
        preset,
        ffmpeg_path: ffmpeg_path.clone(),
    };

    let extension = match video_format {
        VideoFormat::Mp4 => "mp4",
        VideoFormat::WebM => "webm",
    };
    let final_path = output_path(&args.scene, args.output.as_deref(), extension);
    ensure_parent(&final_path)?;
    let temporary = if audio_requested {
        Some(tempfile::tempdir().context("failed to create a video temporary directory")?)
    } else {
        None
    };
    let encode_path = temporary
        .as_ref()
        .map_or_else(|| final_path.clone(), |dir| dir.path().join("silent.mp4"));

    let mut encoder = VideoEncoder::new(&encode_path, video_format, &options)?;
    let mut render_options = render_options(scene, settings, args.scale);
    render_options.target_width = Some(width);
    render_options.target_height = Some(height);

    let total = scene.timeline.frames;
    let progress_step = (total / 10).max(1);
    for frame_index in 0..total {
        if frame_index.is_multiple_of(progress_step) {
            eprintln!(
                "encoding {}: {}/{}",
                format_name(format),
                frame_index + 1,
                total
            );
        }
        let frame = compose_frame(scene, frame_index)?;
        let image = raster::frame_to_rgba(&frame, &render_options)?;
        encoder.write_frame(&image)?;
    }
    encoder.finish()?;

    if let Some(temp) = &temporary {
        let audio_path = if let Some(text) = &args.voice {
            let wav = temp.path().join("voice.wav");
            eprintln!("synthesizing voice-over...");
            generate_voice(text, &wav, Path::new("espeak-ng"))?;
            wav
        } else {
            args.audio.clone().expect("audio_requested implies --audio")
        };
        let audio_options = AudioOptions {
            ffmpeg_path,
            ffprobe_path: resolve_ffprobe_path(&options.ffmpeg_path),
            bitrate_kbps: 192,
        };
        eprintln!("muxing audio...");
        mux_audio(&encode_path, &audio_path, &final_path, &audio_options)?;
    }

    report_video_output(
        json,
        &final_path,
        format,
        scene,
        VideoSummary {
            width,
            height,
            fps,
            audio: audio_requested,
        },
    )
}

#[derive(Debug, Clone, Copy)]
struct VideoSummary {
    width: u32,
    height: u32,
    fps: u16,
    audio: bool,
}

fn video_dimension(value: u32, name: &str) -> Result<u32> {
    if (MIN_VIDEO_DIMENSION..=MAX_VIDEO_DIMENSION).contains(&value) {
        Ok(value)
    } else {
        bail!("video {name} {value} is out of range {MIN_VIDEO_DIMENSION}..={MAX_VIDEO_DIMENSION}");
    }
}

fn resolve_ffmpeg_path(settings: &Settings, args: &RenderArgs) -> PathBuf {
    args.ffmpeg_path
        .clone()
        .or_else(|| settings.config.ffmpeg.path.clone())
        .unwrap_or_else(|| PathBuf::from("ffmpeg"))
}

fn resolve_ffprobe_path(ffmpeg: &Path) -> PathBuf {
    ffmpeg
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(|parent| parent.join("ffprobe"))
        .filter(|candidate| candidate.is_file())
        .unwrap_or_else(|| PathBuf::from("ffprobe"))
}

fn report_video_output(
    json: bool,
    path: &Path,
    format: OutputFormat,
    scene: &Scene,
    summary: VideoSummary,
) -> Result<()> {
    if json {
        print_json(&serde_json::json!({
            "output": path.display().to_string(),
            "format": format_name(format),
            "frames": scene.timeline.frames,
            "fps": summary.fps,
            "width": summary.width,
            "height": summary.height,
            "audio": summary.audio,
        }))
    } else {
        println!("wrote {}", path.display());
        Ok(())
    }
}

/// Long or high-resolution GIFs are slow and huge; warn before writing.
fn warn_if_large_gif(scene: &Scene) {
    const WARN_FRAMES: u32 = 900;
    const WARN_PIXELS: u64 = 250_000_000;

    let pixels = u64::from(scene.output.width) * u64::from(scene.output.height);
    let total_pixels = pixels.saturating_mul(u64::from(scene.timeline.frames));
    if scene.timeline.frames > WARN_FRAMES || total_pixels > WARN_PIXELS {
        eprintln!(
            "warning: this GIF has {} frames at {}x{}; consider MP4 for large animations",
            scene.timeline.frames, scene.output.width, scene.output.height
        );
    }
}

fn run_preview(args: PreviewArgs, json: bool, no_color: bool) -> Result<()> {
    if json {
        bail!("preview does not support --json; use `render --format ansi` for terminal output");
    }
    let scene = load_scene(&args.scene)?;
    validate_scene(&scene)?;
    let color_disabled = no_color || env::var_os("NO_COLOR").is_some();
    tui::run_preview(&scene, args.frame, color_disabled).map_err(anyhow::Error::from)
}

fn validate_scene(scene: &Scene) -> Result<()> {
    if let Err(errors) = scene.validate() {
        let mut message = String::from("scene validation failed:");
        for error in errors {
            message.push_str(&format!("\n  - {error}"));
        }
        bail!(message);
    }
    Ok(())
}

fn load_scene(path: &Path) -> Result<Scene> {
    let contents = fs::read_to_string(path)
        .with_context(|| format!("failed to read scene {}", path.display()))?;
    serde_json::from_str(&contents)
        .with_context(|| format!("failed to parse scene {} as JSON", path.display()))
}

fn output_path(scene: &Path, output: Option<&Path>, extension: &str) -> PathBuf {
    output.map_or_else(
        || scene.with_extension(extension),
        std::path::Path::to_path_buf,
    )
}

fn ensure_parent(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    Ok(())
}

fn report_output(json: bool, path: &Path, format: &str) -> Result<()> {
    if json {
        print_json(&serde_json::json!({
            "output": path.display().to_string(),
            "format": format,
        }))
    } else {
        println!("wrote {}", path.display());
        Ok(())
    }
}

fn write_text_output(output: Option<&Path>, text: &str, json: bool, format: &str) -> Result<()> {
    match output {
        Some(path) => {
            ensure_parent(path)?;
            fs::write(path, text).with_context(|| format!("failed to write {}", path.display()))?;
            report_output(json, path, format)
        }
        None => {
            print!("{text}");
            Ok(())
        }
    }
}

fn format_name(format: OutputFormat) -> &'static str {
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

fn run_fonts(json: bool) -> Result<()> {
    let settings = Settings::load()?;
    let resolution = resolve_font(settings.config.render.font_path.as_deref());

    if json {
        let report = FontsReport {
            fonts: font_names(),
            font_path: resolution.path().map(display_path),
        };
        return print_json(&report);
    }

    println!("Available FIGlet fonts:");
    for font in font_names() {
        println!("  {font}");
    }
    match resolution.path() {
        Some(path) => println!("\nDefault render font: {}", path.display()),
        None => println!("\nDefault render font: not found"),
    }
    Ok(())
}

/// Structured doctor report for MCP and other machine clients.
pub fn doctor_json() -> Result<serde_json::Value> {
    let (settings, config_error) = match Settings::load() {
        Ok(settings) => (settings, None),
        Err(error) => (Settings::default(), Some(format!("{error:#}"))),
    };
    let report = DoctorReport::collect(&settings, config_error.as_deref());
    serde_json::to_value(report).context("could not serialize doctor report")
}

/// Structured font catalog for MCP and other machine clients.
pub fn fonts_json() -> Result<serde_json::Value> {
    let settings = Settings::load()?;
    let resolution = resolve_font(settings.config.render.font_path.as_deref());
    let report = FontsReport {
        fonts: font_names(),
        font_path: resolution.path().map(display_path),
    };
    serde_json::to_value(report).context("could not serialize font catalog")
}

fn run_doctor(json: bool, no_color: bool) -> Result<()> {
    let (settings, config_error) = match Settings::load() {
        Ok(settings) => (settings, None),
        Err(error) => (Settings::default(), Some(format!("{error:#}"))),
    };
    let report = DoctorReport::collect(&settings, config_error.as_deref());

    if json {
        print_json(&report)
    } else {
        report.print_human(color_enabled(no_color));
        Ok(())
    }
}

fn init_tracing() {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("ascii_forge=debug"));
    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr);
    let _ = subscriber.try_init();
}

fn print_json<T: Serialize>(value: &T) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

fn display_path(path: &Path) -> String {
    path.display().to_string()
}

/// Effective configuration after reading `config.toml` and applying environment overrides.
#[derive(Debug, Clone, Default)]
pub struct Settings {
    /// Path of the configuration file, whether or not it exists.
    pub config_path: Option<PathBuf>,
    /// Configuration values with environment overrides applied.
    pub config: Config,
}

impl Settings {
    /// Read the platform config file, if any, and apply environment overrides.
    pub fn load() -> Result<Self> {
        let config_path = config_file_path();
        let mut config = match config_path.as_deref() {
            Some(path) if path.is_file() => read_config(path)?,
            _ => Config::default(),
        };
        config.apply_environment()?;

        let settings = Self {
            config_path,
            config,
        };
        tracing::debug!(config = ?settings.config_path, "configuration loaded");
        Ok(settings)
    }
}

/// Minimal `config.toml` shape. API keys are never stored here.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub ffmpeg: FfmpegConfig,
    pub render: RenderConfig,
    pub ai: AiConfig,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FfmpegConfig {
    /// Path to the `ffmpeg` binary.
    pub path: Option<PathBuf>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RenderConfig {
    /// Path to the monospace TTF used for raster output.
    pub font_path: Option<PathBuf>,
    /// Font size in pixels for raster output.
    pub font_size: f32,
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self {
            font_path: None,
            font_size: RenderOptions::default().font_size,
        }
    }
}

/// AI provider settings. Credentials stay in provider-specific environment variables;
/// the config parser rejects any key it does not know, so an `api_key` never lands here.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AiConfig {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub base_url: Option<String>,
    pub vision_model: Option<String>,
}

impl Config {
    fn apply_environment(&mut self) -> Result<()> {
        if let Some(path) = env_path("ASCII_FORGE_FFMPEG") {
            self.ffmpeg.path = Some(path);
        }
        if let Some(path) = env_path("ASCII_FORGE_FONT") {
            self.render.font_path = Some(path);
        }
        if let Some(raw) = env_value("ASCII_FORGE_FONT_SIZE") {
            self.render.font_size = parse_font_size(&raw)?;
        }
        Ok(())
    }
}

fn read_config(path: &Path) -> Result<Config> {
    let contents = fs::read_to_string(path)
        .with_context(|| format!("failed to read config file {}", path.display()))?;
    toml::from_str(&contents)
        .with_context(|| format!("failed to parse config file {}", path.display()))
}

fn env_value(name: &str) -> Option<String> {
    env::var_os(name)
        .map(|value| value.to_string_lossy().into_owned())
        .filter(|value| !value.trim().is_empty())
}

fn env_path(name: &str) -> Option<PathBuf> {
    env_value(name).map(PathBuf::from)
}

fn parse_font_size(raw: &str) -> Result<f32> {
    let size: f32 = raw
        .trim()
        .parse()
        .with_context(|| format!("ASCII_FORGE_FONT_SIZE must be a number, got `{raw}`"))?;
    if !size.is_finite() || size <= 0.0 {
        bail!("ASCII_FORGE_FONT_SIZE must be a positive number, got `{raw}`");
    }
    Ok(size)
}

fn config_dir() -> Option<PathBuf> {
    ProjectDirs::from("", "", "ascii-forge").map(|dirs| dirs.config_dir().to_path_buf())
}

fn config_file_path() -> Option<PathBuf> {
    config_dir().map(|dir| dir.join("config.toml"))
}

/// How a configurable or default path was resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
enum PathResolution {
    /// The configured path exists.
    Found(PathBuf),
    /// An explicit path was configured but does not exist.
    ConfiguredMissing(PathBuf),
    /// No path was configured and no default candidate exists.
    NotFound,
}

impl PathResolution {
    fn path(&self) -> Option<&Path> {
        match self {
            Self::Found(path) => Some(path),
            Self::ConfiguredMissing(_) | Self::NotFound => None,
        }
    }
}

fn resolve_font(configured: Option<&Path>) -> PathResolution {
    resolve_path(configured, || {
        MONOSPACE_FONT_CANDIDATES
            .iter()
            .map(PathBuf::from)
            .find(|candidate| candidate.is_file())
    })
}

fn resolve_binary(configured: Option<&Path>, name: &str) -> PathResolution {
    resolve_path(configured, || find_on_path(name))
}

/// Explicit configuration wins over defaults; a configured path that does not exist
/// is reported instead of silently falling back.
fn resolve_path(
    configured: Option<&Path>,
    find_default: impl FnOnce() -> Option<PathBuf>,
) -> PathResolution {
    match configured {
        Some(path) if path.is_file() => PathResolution::Found(path.to_path_buf()),
        Some(path) => PathResolution::ConfiguredMissing(path.to_path_buf()),
        None => find_default().map_or(PathResolution::NotFound, PathResolution::Found),
    }
}

fn find_on_path(binary: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    env::split_paths(&path)
        .map(|dir| dir.join(binary))
        .find(|candidate| candidate.is_file())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum CheckStatus {
    Ok,
    Missing,
}

#[derive(Debug, Clone, Serialize)]
struct Check {
    status: CheckStatus,
    detail: String,
}

impl Check {
    fn ok(detail: impl Into<String>) -> Self {
        Self {
            status: CheckStatus::Ok,
            detail: detail.into(),
        }
    }

    fn missing(detail: impl Into<String>) -> Self {
        Self {
            status: CheckStatus::Missing,
            detail: detail.into(),
        }
    }
}

#[derive(Debug, Serialize)]
struct FontsReport {
    fonts: &'static [&'static str],
    font_path: Option<String>,
}

#[derive(Debug, Serialize)]
struct DoctorReport {
    ffmpeg: Check,
    ffprobe: Check,
    libx264: Check,
    libvpx_vp9: Check,
    font: Check,
    config_dir: Check,
}

impl DoctorReport {
    fn collect(settings: &Settings, config_error: Option<&str>) -> Self {
        let ffmpeg = resolve_binary(settings.config.ffmpeg.path.as_deref(), "ffmpeg");
        let ffprobe = resolve_binary(None, "ffprobe");
        let (libx264, libvpx_vp9) = check_encoders(ffmpeg.path());
        let font = resolve_font(settings.config.render.font_path.as_deref());

        Self {
            ffmpeg: check_binary(&ffmpeg, "ffmpeg"),
            ffprobe: check_binary(&ffprobe, "ffprobe"),
            libx264,
            libvpx_vp9,
            font: check_font(&font),
            config_dir: check_config_dir(settings, config_error),
        }
    }

    fn print_human(&self, color: bool) {
        println!("ascii-forge doctor");
        print_check("ffmpeg", &self.ffmpeg, color);
        print_check("ffprobe", &self.ffprobe, color);
        print_check("libx264", &self.libx264, color);
        print_check("libvpx-vp9", &self.libvpx_vp9, color);
        print_check("font", &self.font, color);
        print_check("config_dir", &self.config_dir, color);
    }
}

fn check_binary(resolution: &PathResolution, name: &str) -> Check {
    match resolution {
        PathResolution::Found(path) => Check::ok(display_path(path)),
        PathResolution::ConfiguredMissing(path) => {
            Check::missing(format!("configured {name} not found: {}", path.display()))
        }
        PathResolution::NotFound => Check::missing(format!("{name} is not on PATH")),
    }
}

fn check_font(resolution: &PathResolution) -> Check {
    match resolution {
        PathResolution::Found(path) => Check::ok(display_path(path)),
        PathResolution::ConfiguredMissing(path) => {
            Check::missing(format!("configured font not found: {}", path.display()))
        }
        PathResolution::NotFound => Check::missing(
            "no known monospace font found; set ASCII_FORGE_FONT or [render].font_path",
        ),
    }
}

fn check_encoders(ffmpeg: Option<&Path>) -> (Check, Check) {
    let Some(ffmpeg) = ffmpeg else {
        return encoder_check_failure("ffmpeg is required to list encoders");
    };

    match ProcessCommand::new(ffmpeg)
        .args(["-hide_banner", "-encoders"])
        .output()
    {
        Ok(output) if output.status.success() => {
            let encoders = String::from_utf8_lossy(&output.stdout);
            (
                check_encoder(&encoders, "libx264"),
                check_encoder(&encoders, "libvpx-vp9"),
            )
        }
        Ok(_) => encoder_check_failure("`ffmpeg -encoders` failed"),
        Err(error) => encoder_check_failure(&format!("failed to run ffmpeg: {error}")),
    }
}

fn encoder_check_failure(detail: &str) -> (Check, Check) {
    (Check::missing(detail), Check::missing(detail))
}

fn check_encoder(encoders: &str, encoder: &str) -> Check {
    if encoder_listed(encoders, encoder) {
        Check::ok("available")
    } else {
        Check::missing("not listed by `ffmpeg -encoders`")
    }
}

fn encoder_listed(encoders: &str, encoder: &str) -> bool {
    encoders
        .lines()
        .filter_map(|line| line.split_whitespace().nth(1))
        .any(|name| name == encoder)
}

fn check_config_dir(settings: &Settings, config_error: Option<&str>) -> Check {
    if let Some(error) = config_error {
        return Check::missing(error);
    }
    let Some(config_path) = settings.config_path.as_deref() else {
        return Check::missing("config directory is unavailable on this platform");
    };
    let Some(dir) = config_path.parent() else {
        return Check::missing(format!(
            "cannot derive config directory from {}",
            config_path.display()
        ));
    };
    check_writable(dir)
}

fn check_writable(path: &Path) -> Check {
    if let Err(error) = fs::create_dir_all(path) {
        return Check::missing(format!("cannot create {}: {error}", path.display()));
    }
    let probe = path.join(format!(".write-probe-{}", process::id()));
    let result = fs::write(&probe, b"");
    let _ = fs::remove_file(&probe);

    match result {
        Ok(()) => Check::ok(display_path(path)),
        Err(error) => Check::missing(format!("{} is not writable: {error}", path.display())),
    }
}

fn print_check(label: &str, check: &Check, color: bool) {
    let marker = match check.status {
        CheckStatus::Ok => "ok",
        CheckStatus::Missing => "missing",
    };
    let marker = paint_marker(marker, check.status, color);
    println!("[{marker}] {label}: {}", check.detail);
}

fn paint_marker(marker: &str, status: CheckStatus, enabled: bool) -> String {
    if !enabled {
        return marker.to_string();
    }
    let code = match status {
        CheckStatus::Ok => "32",
        CheckStatus::Missing => "31",
    };
    format!("\x1b[{code}m{marker}\x1b[0m")
}

fn color_enabled(no_color: bool) -> bool {
    !no_color && env::var_os("NO_COLOR").is_none() && std::io::stdout().is_terminal()
}
