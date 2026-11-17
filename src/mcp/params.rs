//! Tool parameter types and their JSON schemas.
//!
//! Unknown fields are rejected so typos fail loudly instead of being ignored.

use schemars::JsonSchema;
use serde::Deserialize;

use crate::ai::TemplateKind;

/// Offline template family. Template mode never touches the network.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TemplateName {
    Logo,
    Promo,
    Glitch,
    Matrix,
    LowerThird,
}

impl TemplateName {
    /// Stable wire name used in tool results.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Logo => "logo",
            Self::Promo => "promo",
            Self::Glitch => "glitch",
            Self::Matrix => "matrix",
            Self::LowerThird => "lower_third",
        }
    }

    /// The plan compiler's template kind.
    pub const fn kind(self) -> TemplateKind {
        match self {
            Self::Logo => TemplateKind::Logo,
            Self::Promo => TemplateKind::Promo,
            Self::Glitch => TemplateKind::Glitch,
            Self::Matrix => TemplateKind::Matrix,
            Self::LowerThird => TemplateKind::LowerThird,
        }
    }
}

impl From<TemplateName> for TemplateKind {
    fn from(value: TemplateName) -> Self {
        value.kind()
    }
}

/// Parameters for `create_scene`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateSceneParams {
    /// Natural-language scene description for the AI provider. Use either
    /// `prompt` or `template`, never both.
    #[serde(default)]
    pub prompt: Option<String>,

    /// Offline template used instead of an AI prompt.
    #[serde(default)]
    pub template: Option<TemplateName>,

    /// Title override; required and non-blank in template mode.
    #[serde(default)]
    pub title: Option<String>,

    /// Optional tagline; a blank value removes the tagline.
    #[serde(default)]
    pub tagline: Option<String>,

    /// Deterministic seed. Defaults to 0 in template mode and to the model's
    /// choice in prompt mode.
    #[serde(default)]
    pub seed: Option<u64>,

    /// Project-relative scene path. Defaults to
    /// `.ascii-forge/artifacts/scenes/<slug>-<seed>.json`.
    #[serde(default)]
    pub out: Option<String>,

    /// Replace an existing scene file instead of failing with `output_exists`.
    #[serde(default)]
    pub overwrite: bool,

    /// Bypass the project response cache in prompt mode.
    #[serde(default)]
    pub no_cache: bool,
}

/// Preview encoding.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PreviewFormat {
    /// Plain characters with no escape sequences.
    #[default]
    Text,
    /// Truecolor SGR escapes, still bounded to the requested cells.
    Ansi,
}

impl PreviewFormat {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Ansi => "ansi",
        }
    }
}

/// Parameters for `render_preview`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RenderPreviewParams {
    /// Project-relative scene file written by `create_scene`.
    pub scene_path: String,

    /// Frame index to preview; defaults to the first frame.
    #[serde(default)]
    pub frame: Option<u32>,

    /// Maximum preview columns; defaults to 120 and is capped at 500.
    #[serde(default)]
    pub max_cols: Option<u16>,

    /// Maximum preview rows; defaults to 30 and is capped at 200.
    #[serde(default)]
    pub max_rows: Option<u16>,

    /// Preview encoding; text by default.
    #[serde(default)]
    pub format: Option<PreviewFormat>,
}

/// Artifact formats supported by `export`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExportFormat {
    Png,
    Jpeg,
    Svg,
    Gif,
    Mp4,
    /// Explicit rename because `snake_case` would spell it `web_m`.
    #[serde(rename = "webm")]
    WebM,
    Ansi,
    Text,
}

impl ExportFormat {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpeg",
            Self::Svg => "svg",
            Self::Gif => "gif",
            Self::Mp4 => "mp4",
            Self::WebM => "webm",
            Self::Ansi => "ansi",
            Self::Text => "text",
        }
    }

    /// File extension, including JPEG's conventional `.jpg`.
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Jpeg => "jpg",
            _ => self.as_str(),
        }
    }

    pub const fn mime_type(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
            Self::Svg => "image/svg+xml",
            Self::Gif => "image/gif",
            Self::Mp4 => "video/mp4",
            Self::WebM => "video/webm",
            Self::Ansi | Self::Text => "text/plain",
        }
    }

    /// Formats that encode every timeline frame instead of one frame.
    pub const fn is_animated(self) -> bool {
        matches!(self, Self::Gif | Self::Mp4 | Self::WebM)
    }
}

/// Parameters for `export`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExportParams {
    /// Project-relative scene file written by `create_scene`.
    pub scene_path: String,

    /// Artifact format to write.
    pub format: ExportFormat,

    /// Project-relative output path under `.ascii-forge/artifacts/`. Defaults
    /// to `.ascii-forge/artifacts/exports/<scene-stem>.<ext>`.
    #[serde(default)]
    pub out: Option<String>,

    /// Replace an existing artifact instead of failing with `output_exists`.
    #[serde(default)]
    pub overwrite: bool,

    /// Frame index for static formats. Animated formats always use the whole
    /// timeline.
    #[serde(default)]
    pub frame: Option<u32>,

    /// GIF palette size, from 2 to 256 colors.
    #[serde(default)]
    pub gif_colors: Option<u16>,

    /// Video quality (CRF); lower is better and larger.
    #[serde(default)]
    pub crf: Option<u8>,

    /// Encoder preset; x264 names, mapped to `-cpu-used` for VP9.
    #[serde(default)]
    pub preset: Option<String>,
}
