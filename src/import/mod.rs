//! Image and video import to ASCII grids and scenes.
//!
//! Image conversion is pure and deterministic: resize, luminance mapping,
//! optional color sampling, braille dots, Floyd-Steinberg dithering or Sobel
//! edges. Video decoding lives in [`video`].

pub mod video;

use std::path::{Path, PathBuf};

use image::imageops::FilterType;
use image::{DynamicImage, GenericImageView, RgbaImage};
use thiserror::Error;

use crate::scene::{
    Canvas, Cell, Grid, GridLayer, Layer, LayerKind, OutputFormat, OutputSpec, Rgba, SCENE_VERSION,
    Scene, Timeline, Transform, ValidationError,
};

/// Default luminance ramp, dark to bright.
pub const ASCII_CHARSET: &str = " .:-=+*#%@";
/// Block-character ramp, dark to bright.
pub const BLOCKS_CHARSET: &str = " ░▒▓█";
/// Denser ramp for detailed images.
pub const DETAILED_CHARSET: &str = " .,:;irsXA253hMHGS#9B&@";

/// Default target columns.
pub const DEFAULT_COLS: u16 = 80;
/// Default cell width/height ratio used for aspect correction.
pub const DEFAULT_CELL_ASPECT: f32 = 0.542;
/// Default video sampling rate.
pub const DEFAULT_FPS: u16 = 12;
/// Default maximum embedded video frames.
pub const DEFAULT_MAX_FRAMES: u32 = 120;
/// Maximum columns accepted by the importer.
pub const MAX_COLS: u16 = 512;
/// Maximum rows accepted by the importer.
pub const MAX_ROWS: u16 = 512;
/// Maximum frames accepted in one imported video.
pub const MAX_FRAMES: u32 = 1000;
/// Maximum total cells embedded in one imported video scene.
pub const MAX_IMPORTED_CELLS: u64 = 2_000_000;
/// Pixel size used to derive a static scene's output dimensions.
const OUTPUT_CELL_PIXELS: u32 = 16;
/// Braille dot positions: (dx, dy, bit) for dots 1..8.
const BRAILLE_DOTS: [(u16, u16, u16); 8] = [
    (0, 0, 0x01),
    (0, 1, 0x02),
    (0, 2, 0x04),
    (1, 0, 0x08),
    (1, 1, 0x10),
    (1, 2, 0x20),
    (0, 3, 0x40),
    (1, 3, 0x80),
];

#[derive(Debug, Error)]
pub enum ImportError {
    #[error("failed to open image {path}: {source}")]
    ImageOpen {
        path: PathBuf,
        #[source]
        source: image::ImageError,
    },

    #[error("invalid import options: {0}")]
    Options(String),

    #[error("scene validation failed: {0}")]
    Scene(String),

    #[error("video decoder failed: {0}")]
    Video(String),
}

/// Options shared by image and video import.
#[derive(Debug, Clone)]
pub struct ImportOptions {
    pub cols: u16,
    pub rows: Option<u16>,
    pub charset: String,
    pub color: bool,
    pub braille: bool,
    pub dither: bool,
    pub invert: bool,
    pub edges: bool,
    pub background: Rgba,
    pub foreground: Rgba,
    pub cell_aspect: f32,
    pub fps: u16,
    pub max_frames: u32,
}

impl Default for ImportOptions {
    fn default() -> Self {
        Self {
            cols: DEFAULT_COLS,
            rows: None,
            charset: ASCII_CHARSET.to_string(),
            color: false,
            braille: false,
            dither: false,
            invert: false,
            edges: false,
            background: Rgba::rgb(0, 0, 0),
            foreground: Rgba::rgb(255, 255, 255),
            cell_aspect: DEFAULT_CELL_ASPECT,
            fps: DEFAULT_FPS,
            max_frames: DEFAULT_MAX_FRAMES,
        }
    }
}

impl ImportOptions {
    /// Validate ranges and mutually exclusive conversion modes.
    pub fn validate(&self) -> Result<(), ImportError> {
        if !(1..=MAX_COLS).contains(&self.cols) {
            return Err(ImportError::Options(format!(
                "columns {} out of range 1..={MAX_COLS}",
                self.cols
            )));
        }
        if let Some(rows) = self.rows
            && !(1..=MAX_ROWS).contains(&rows)
        {
            return Err(ImportError::Options(format!(
                "rows {rows} out of range 1..={MAX_ROWS}"
            )));
        }
        if !(1..=60).contains(&self.fps) {
            return Err(ImportError::Options(format!(
                "fps {} out of range 1..=60",
                self.fps
            )));
        }
        if !(1..=MAX_FRAMES).contains(&self.max_frames) {
            return Err(ImportError::Options(format!(
                "max frames {} out of range 1..={MAX_FRAMES}",
                self.max_frames
            )));
        }
        if !(0.2..=2.0).contains(&self.cell_aspect) {
            return Err(ImportError::Options(format!(
                "cell aspect {} out of range 0.2..=2.0",
                self.cell_aspect
            )));
        }
        if self.braille && self.dither {
            return Err(ImportError::Options(
                "--braille and --dither cannot be combined".to_string(),
            ));
        }
        if self.braille && self.edges {
            return Err(ImportError::Options(
                "--braille and --edges cannot be combined".to_string(),
            ));
        }
        if self.dither && self.edges {
            return Err(ImportError::Options(
                "--dither and --edges cannot be combined".to_string(),
            ));
        }
        if !self.braille && !self.edges {
            let chars = self.charset.chars().count();
            if chars < 2 {
                return Err(ImportError::Options(
                    "charset must contain at least two characters".to_string(),
                ));
            }
        }
        Ok(())
    }
}

/// Resolve a charset preset or accept a custom ramp.
pub fn resolve_charset(value: Option<&str>) -> Result<String, ImportError> {
    let Some(value) = value else {
        return Ok(ASCII_CHARSET.to_string());
    };
    let preset = value.trim();
    if preset.is_empty() {
        return Err(ImportError::Options(
            "charset must not be empty".to_string(),
        ));
    }
    Ok(match preset {
        "ascii" | "default" => ASCII_CHARSET.to_string(),
        "blocks" => BLOCKS_CHARSET.to_string(),
        "detailed" => DETAILED_CHARSET.to_string(),
        _ => value.to_string(),
    })
}

/// Number of rows that preserves the source aspect with the given cell aspect.
pub fn rows_for(cols: u16, width: u32, height: u32, cell_aspect: f32) -> u16 {
    if width == 0 || height == 0 {
        return 1;
    }
    let rows = f32::from(cols) * cell_aspect * height as f32 / width as f32;
    rows.round().clamp(1.0, f32::from(MAX_ROWS)) as u16
}

/// Import a still image into an ASCII grid.
pub fn import_image(path: &Path, options: &ImportOptions) -> Result<Grid, ImportError> {
    options.validate()?;
    let image = image::open(path).map_err(|source| ImportError::ImageOpen {
        path: path.to_path_buf(),
        source,
    })?;
    image_to_grid(&image, options)
}

/// Convert any decoded image into an ASCII grid.
pub fn image_to_grid(image: &DynamicImage, options: &ImportOptions) -> Result<Grid, ImportError> {
    options.validate()?;
    let (source_width, source_height) = image.dimensions();
    if source_width == 0 || source_height == 0 {
        return Err(ImportError::Options(
            "image has zero width or height".to_string(),
        ));
    }
    let rows = options.rows.unwrap_or_else(|| {
        rows_for(
            options.cols,
            source_width,
            source_height,
            options.cell_aspect,
        )
    });

    if options.braille {
        braille_grid(image, options.cols, rows, options)
    } else {
        character_grid(image, options.cols, rows, options)
    }
}

/// Build a one-frame PNG scene from an imported grid.
pub fn build_static_scene(grid: Grid, options: &ImportOptions) -> Result<Scene, ImportError> {
    options.validate()?;
    let (width, height) = output_dimensions(grid.cols, grid.rows, options.cell_aspect);
    let scene = Scene {
        version: SCENE_VERSION,
        seed: 0,
        canvas: Canvas {
            cols: grid.cols,
            rows: grid.rows,
            cell_aspect: options.cell_aspect,
            background: options.background,
            foreground: options.foreground,
        },
        layers: vec![Layer {
            id: "imported".to_string(),
            kind: LayerKind::Grid(GridLayer { grid }),
            transform: Transform::default(),
            keyframes: Vec::new(),
        }],
        timeline: Timeline {
            fps: 1,
            frames: 1,
            loop_: false,
        },
        effects: Vec::new(),
        output: OutputSpec {
            width,
            height,
            format: OutputFormat::Png,
        },
    };
    scene.validate().map_err(scene_errors)?;
    Ok(scene)
}

/// Build an animated GIF scene from imported frames.
pub fn build_video_scene(
    frames: Vec<Grid>,
    fps: u16,
    options: &ImportOptions,
) -> Result<Scene, ImportError> {
    options.validate()?;
    if !(1..=60).contains(&fps) {
        return Err(ImportError::Options(format!(
            "fps {fps} out of range 1..=60"
        )));
    }
    let Some(first) = frames.first() else {
        return Err(ImportError::Video("video produced no frames".to_string()));
    };
    let cols = first.cols;
    let rows = first.rows;
    if frames
        .iter()
        .any(|frame| frame.cols != cols || frame.rows != rows)
    {
        return Err(ImportError::Video(
            "decoded frames have inconsistent dimensions".to_string(),
        ));
    }
    let frame_count = u32::try_from(frames.len())
        .map_err(|_| ImportError::Video("too many decoded frames".to_string()))?;
    let total_cells = u64::from(cols) * u64::from(rows) * u64::from(frame_count);
    if total_cells > MAX_IMPORTED_CELLS {
        return Err(ImportError::Video(format!(
            "import would embed {total_cells} cells; reduce --cols, --rows or --max-frames"
        )));
    }
    let (width, height) = output_dimensions(cols, rows, options.cell_aspect);
    let scene = Scene {
        version: SCENE_VERSION,
        seed: 0,
        canvas: Canvas {
            cols,
            rows,
            cell_aspect: options.cell_aspect,
            background: options.background,
            foreground: options.foreground,
        },
        layers: vec![Layer {
            id: "imported".to_string(),
            kind: LayerKind::Frames(crate::scene::FramesLayer { frames }),
            transform: Transform::default(),
            keyframes: Vec::new(),
        }],
        timeline: Timeline {
            fps,
            frames: frame_count,
            loop_: true,
        },
        effects: Vec::new(),
        output: OutputSpec {
            width,
            height,
            format: OutputFormat::Gif,
        },
    };
    scene.validate().map_err(scene_errors)?;
    Ok(scene)
}

fn character_grid(
    image: &DynamicImage,
    cols: u16,
    rows: u16,
    options: &ImportOptions,
) -> Result<Grid, ImportError> {
    let resized = image::imageops::resize(
        &image.to_rgba8(),
        u32::from(cols),
        u32::from(rows),
        FilterType::Triangle,
    );
    let mut values = Vec::with_capacity(usize::from(cols) * usize::from(rows));
    for pixel in resized.pixels() {
        let color = blend_pixel(*pixel, options.background);
        let mut value = luminance(color);
        if options.invert {
            value = 1.0 - value;
        }
        values.push(value);
    }
    let charset: Vec<char> = options.charset.chars().collect();
    if options.dither {
        let levels = charset.len().saturating_sub(1).max(1) as f32;
        floyd_steinberg(&mut values, cols, rows, levels);
    }
    let edges = options.edges.then(|| sobel(&values, cols, rows));
    let mut grid = Grid::blank(cols, rows, options.foreground, Rgba::TRANSPARENT);
    for y in 0..rows {
        for x in 0..cols {
            let index = usize::from(y) * usize::from(cols) + usize::from(x);
            let ch = if let Some(edges) = &edges {
                edge_glyph(edges[index])
            } else {
                ramp_glyph(values[index], &charset)
            };
            let fg = if options.color {
                blend_pixel(
                    *resized.get_pixel(u32::from(x), u32::from(y)),
                    options.background,
                )
            } else {
                options.foreground
            };
            if let Some(cell) = grid.get_mut(x, y) {
                *cell = Cell {
                    ch,
                    fg,
                    bg: Rgba::TRANSPARENT,
                };
            }
        }
    }
    Ok(grid)
}

fn braille_grid(
    image: &DynamicImage,
    cols: u16,
    rows: u16,
    options: &ImportOptions,
) -> Result<Grid, ImportError> {
    let dot_cols = u32::from(cols) * 2;
    let dot_rows = u32::from(rows) * 4;
    let dots = image::imageops::resize(&image.to_rgba8(), dot_cols, dot_rows, FilterType::Triangle);
    let cell_colors = image::imageops::resize(
        &image.to_rgba8(),
        u32::from(cols),
        u32::from(rows),
        FilterType::Triangle,
    );
    let mut grid = Grid::blank(cols, rows, options.foreground, Rgba::TRANSPARENT);

    for y in 0..rows {
        for x in 0..cols {
            let mut bits = 0_u16;
            for (dx, dy, bit) in BRAILLE_DOTS {
                let px = u32::from(x) * 2 + u32::from(dx);
                let py = u32::from(y) * 4 + u32::from(dy);
                let color = blend_pixel(*dots.get_pixel(px, py), options.background);
                let mut value = luminance(color);
                if options.invert {
                    value = 1.0 - value;
                }
                if value < 0.5 {
                    bits |= bit;
                }
            }
            let ch = char::from_u32(0x2800 + u32::from(bits))
                .expect("braille codepoint is always valid");
            let fg = if options.color {
                blend_pixel(
                    *cell_colors.get_pixel(u32::from(x), u32::from(y)),
                    options.background,
                )
            } else {
                options.foreground
            };
            if let Some(cell) = grid.get_mut(x, y) {
                *cell = Cell {
                    ch,
                    fg,
                    bg: Rgba::TRANSPARENT,
                };
            }
        }
    }
    Ok(grid)
}

fn ramp_glyph(value: f32, charset: &[char]) -> char {
    let last = charset.len().saturating_sub(1);
    let index = (value.clamp(0.0, 1.0) * last as f32).round() as usize;
    charset[index.min(last)]
}

fn floyd_steinberg(values: &mut [f32], cols: u16, rows: u16, levels: f32) {
    for y in 0..usize::from(rows) {
        for x in 0..usize::from(cols) {
            let index = y * usize::from(cols) + x;
            let old = values[index].clamp(0.0, 1.0);
            let new = (old * levels).round() / levels;
            values[index] = new;
            let error = old - new;
            if x + 1 < usize::from(cols) {
                values[index + 1] += error * 7.0 / 16.0;
            }
            if y + 1 < usize::from(rows) {
                let next = index + usize::from(cols);
                if x > 0 {
                    values[next - 1] += error * 3.0 / 16.0;
                }
                values[next] += error * 5.0 / 16.0;
                if x + 1 < usize::from(cols) {
                    values[next + 1] += error * 1.0 / 16.0;
                }
            }
        }
    }
}

fn sobel(values: &[f32], cols: u16, rows: u16) -> Vec<(f32, f32)> {
    let width = usize::from(cols);
    let height = usize::from(rows);
    let at = |x: isize, y: isize| -> f32 {
        let x = x.clamp(0, width.saturating_sub(1) as isize) as usize;
        let y = y.clamp(0, height.saturating_sub(1) as isize) as usize;
        values[y * width + x]
    };
    let mut result = Vec::with_capacity(width * height);
    for y in 0..height {
        for x in 0..width {
            let x = x as isize;
            let y = y as isize;
            let gx = -at(x - 1, y - 1) + at(x + 1, y - 1) - 2.0 * at(x - 1, y) + 2.0 * at(x + 1, y)
                - at(x - 1, y + 1)
                + at(x + 1, y + 1);
            let gy = -at(x - 1, y - 1) - 2.0 * at(x, y - 1) - at(x + 1, y - 1)
                + at(x - 1, y + 1)
                + 2.0 * at(x, y + 1)
                + at(x + 1, y + 1);
            result.push((gx, gy));
        }
    }
    result
}

fn edge_glyph((gx, gy): (f32, f32)) -> char {
    let magnitude = gx.hypot(gy);
    if magnitude < 0.15 {
        return ' ';
    }
    let angle = gy.atan2(gx).to_degrees().rem_euclid(180.0);
    if angle < 22.5 || angle >= 157.5 {
        '|'
    } else if angle < 67.5 {
        '/'
    } else if angle < 112.5 {
        '-'
    } else {
        '\\'
    }
}

fn blend_pixel(pixel: image::Rgba<u8>, background: Rgba) -> Rgba {
    let alpha = f32::from(pixel[3]) / 255.0;
    let channel = |source: u8, backdrop: u8| {
        (f32::from(source) * alpha + f32::from(backdrop) * (1.0 - alpha)).round() as u8
    };
    Rgba::new(
        channel(pixel[0], background.r),
        channel(pixel[1], background.g),
        channel(pixel[2], background.b),
        255,
    )
}

fn luminance(color: Rgba) -> f32 {
    (0.2126 * f32::from(color.r) + 0.7152 * f32::from(color.g) + 0.0722 * f32::from(color.b))
        / 255.0
}

fn output_dimensions(cols: u16, rows: u16, cell_aspect: f32) -> (u32, u32) {
    let width = (u32::from(cols) * OUTPUT_CELL_PIXELS).clamp(16, 8192);
    let height = ((f32::from(rows) * OUTPUT_CELL_PIXELS as f32 / cell_aspect).round() as u32)
        .clamp(16, 8192);
    (width, height)
}

fn scene_errors(errors: Vec<ValidationError>) -> ImportError {
    ImportError::Scene(
        errors
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("; "),
    )
}

/// Resize helper used by video import when a frame is already RGBA.
pub(crate) fn rgba_image_to_grid(
    image: RgbaImage,
    options: &ImportOptions,
) -> Result<Grid, ImportError> {
    image_to_grid(&DynamicImage::ImageRgba8(image), options)
}
