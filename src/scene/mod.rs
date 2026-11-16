//! Scene IR shared by the renderer, the AI compiler and the exporters.

pub mod compose;
pub mod validate;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub use validate::ValidationError;

/// Current scene schema version.
pub const SCENE_VERSION: u32 = 1;

/// Straight RGBA color used by cells, layers and backgrounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    /// Fully transparent black, used by layer grids that must not erase the canvas.
    pub const TRANSPARENT: Self = Self::new(0, 0, 0, 0);

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub const fn to_array(self) -> [u8; 4] {
        [self.r, self.g, self.b, self.a]
    }

    /// Linear interpolation from `self` to `other`; `t` is clamped to 0..=1.
    pub fn blend(self, other: Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        let channel = |from: u8, to: u8| {
            (f32::from(from) + (f32::from(to) - f32::from(from)) * t).round() as u8
        };
        Self {
            r: channel(self.r, other.r),
            g: channel(self.g, other.g),
            b: channel(self.b, other.b),
            a: channel(self.a, other.a),
        }
    }
}

impl Default for Rgba {
    fn default() -> Self {
        Self::rgb(255, 255, 255)
    }
}

/// A versioned ASCII scene.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Scene {
    pub version: u32,
    /// Deterministic seed for effects and procedural variation.
    #[serde(default)]
    pub seed: u64,
    pub canvas: Canvas,
    pub layers: Vec<Layer>,
    pub timeline: Timeline,
    #[serde(default)]
    pub effects: Vec<Effect>,
    #[serde(default)]
    pub output: OutputSpec,
}

impl Scene {
    /// Validate the scene against the current schema and semantic rules.
    pub fn validate(&self) -> Result<(), Vec<ValidationError>> {
        validate::validate(self)
    }
}

/// Grid geometry and default colors.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Canvas {
    pub cols: u16,
    pub rows: u16,
    #[serde(default = "default_cell_aspect")]
    pub cell_aspect: f32,
    #[serde(default = "default_background")]
    pub background: Rgba,
    #[serde(default = "default_foreground")]
    pub foreground: Rgba,
}

/// A layer in the scene.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Layer {
    pub id: String,
    pub kind: LayerKind,
    #[serde(default)]
    pub transform: Transform,
    #[serde(default)]
    pub keyframes: Vec<Keyframe>,
}

/// The supported layer kinds.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LayerKind {
    Text(TextLayer),
    Figlet(FigletLayer),
    Image(ImageLayer),
    Grid(GridLayer),
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TextLayer {
    pub text: String,
    #[serde(default)]
    pub color: Option<Rgba>,
    #[serde(default)]
    pub align: Align,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FigletLayer {
    pub text: String,
    #[serde(default = "default_figlet_font")]
    pub font: String,
    #[serde(default)]
    pub color: Option<Rgba>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ImageLayer {
    pub source: String,
    #[serde(default)]
    pub charset: Option<String>,
    #[serde(default)]
    pub color: bool,
    #[serde(default)]
    pub braille: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GridLayer {
    pub grid: Grid,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Transform {
    #[serde(default)]
    pub x: i32,
    #[serde(default)]
    pub y: i32,
    #[serde(default = "default_scale")]
    pub scale: f32,
    #[serde(default = "default_opacity")]
    pub opacity: f32,
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            x: 0,
            y: 0,
            scale: 1.0,
            opacity: 1.0,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
pub struct Keyframe {
    pub frame: u32,
    pub transform: Transform,
    #[serde(default)]
    pub easing: Easing,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Easing {
    #[default]
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
    CubicBezier {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
pub struct Timeline {
    #[serde(default = "default_fps")]
    pub fps: u16,
    #[serde(default = "default_frames")]
    pub frames: u32,
    #[serde(default, rename = "loop")]
    pub loop_: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Effect {
    pub id: String,
    pub kind: EffectKind,
    #[serde(default)]
    pub target: Option<String>,
    pub start: u32,
    pub end: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EffectKind {
    Typewriter {
        #[serde(default = "default_chars_per_frame")]
        chars_per_frame: f32,
    },
    Fade {
        #[serde(default)]
        from: f32,
        #[serde(default = "default_opacity")]
        to: f32,
    },
    Slide {
        dx: i32,
        dy: i32,
    },
    Scanline {
        #[serde(default = "default_intensity")]
        intensity: f32,
        #[serde(default = "default_speed")]
        speed: f32,
    },
    Glitch {
        #[serde(default = "default_intensity")]
        intensity: f32,
        #[serde(default)]
        seed: u64,
    },
    Wave {
        #[serde(default = "default_amplitude")]
        amplitude: f32,
        #[serde(default = "default_frequency")]
        frequency: f32,
        #[serde(default = "default_speed")]
        speed: f32,
    },
    Dissolve {
        #[serde(default)]
        seed: u64,
    },
    Blink {
        #[serde(default = "default_period")]
        period: u32,
    },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OutputFormat {
    #[default]
    Png,
    Jpeg,
    Svg,
    Gif,
    Mp4,
    WebM,
    Ansi,
    Text,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
pub struct OutputSpec {
    #[serde(default = "default_width")]
    pub width: u32,
    #[serde(default = "default_height")]
    pub height: u32,
    #[serde(default)]
    pub format: OutputFormat,
}

impl Default for OutputSpec {
    fn default() -> Self {
        Self {
            width: default_width(),
            height: default_height(),
            format: OutputFormat::Png,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Align {
    Left,
    #[default]
    Center,
    Right,
}

/// One character cell with foreground and background colors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Cell {
    pub ch: char,
    pub fg: Rgba,
    pub bg: Rgba,
}

/// Dense row-major grid of cells.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Grid {
    pub cols: u16,
    pub rows: u16,
    pub cells: Vec<Cell>,
}

impl Grid {
    pub fn new(cols: u16, rows: u16, cell: Cell) -> Self {
        let len = cols as usize * rows as usize;
        Self {
            cols,
            rows,
            cells: vec![cell; len],
        }
    }

    pub fn blank(cols: u16, rows: u16, fg: Rgba, bg: Rgba) -> Self {
        Self::new(cols, rows, Cell { ch: ' ', fg, bg })
    }

    pub fn index(&self, x: u16, y: u16) -> Option<usize> {
        if x >= self.cols || y >= self.rows {
            return None;
        }
        Some(y as usize * self.cols as usize + x as usize)
    }

    pub fn get(&self, x: u16, y: u16) -> Option<&Cell> {
        self.index(x, y).map(|index| &self.cells[index])
    }

    pub fn get_mut(&mut self, x: u16, y: u16) -> Option<&mut Cell> {
        self.index(x, y).map(|index| &mut self.cells[index])
    }

    pub fn set(&mut self, x: u16, y: u16, cell: Cell) -> bool {
        match self.index(x, y) {
            Some(index) => {
                self.cells[index] = cell;
                true
            }
            None => false,
        }
    }

    pub fn fill(&mut self, cell: Cell) {
        self.cells.fill(cell);
    }

    pub fn row(&self, y: u16) -> Option<&[Cell]> {
        if y >= self.rows {
            return None;
        }
        let start = y as usize * self.cols as usize;
        Some(&self.cells[start..start + self.cols as usize])
    }

    pub fn to_text(&self) -> String {
        let mut out = String::with_capacity((self.cols as usize + 1) * self.rows as usize);
        for y in 0..self.rows {
            if let Some(row) = self.row(y) {
                for cell in row {
                    out.push(cell.ch);
                }
            }
            out.push('\n');
        }
        out
    }
}

/// A single rendered frame.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Frame {
    pub index: u32,
    pub grid: Grid,
}

fn default_cell_aspect() -> f32 {
    0.542
}

fn default_background() -> Rgba {
    Rgba::rgb(10, 12, 16)
}

fn default_foreground() -> Rgba {
    Rgba::rgb(0, 255, 136)
}

fn default_figlet_font() -> String {
    "standard".to_string()
}

fn default_scale() -> f32 {
    1.0
}

fn default_opacity() -> f32 {
    1.0
}

fn default_fps() -> u16 {
    30
}

fn default_frames() -> u32 {
    1
}

fn default_chars_per_frame() -> f32 {
    1.0
}

fn default_intensity() -> f32 {
    0.5
}

fn default_speed() -> f32 {
    1.0
}

fn default_amplitude() -> f32 {
    1.0
}

fn default_frequency() -> f32 {
    0.1
}

fn default_period() -> u32 {
    30
}

fn default_width() -> u32 {
    1920
}

fn default_height() -> u32 {
    1080
}
