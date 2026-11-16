//! Streaming FFmpeg video export.
//!
//! Frames are written to an external FFmpeg process through its stdin as raw
//! RGBA bytes as soon as they are rendered, so an animation is never collected
//! in memory and no PNG intermediates are created.
//!
//! FFmpeg is a runtime dependency, not a linked library: the encoder spawns the
//! binary at [`VideoOptions::ffmpeg_path`] and reports its stderr on failure.
//! Odd canvas sizes are accepted and padded to even dimensions because the
//! `yuv420p` pixel format requires even width and height.

use std::fmt;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};

use image::RgbaImage;
use thiserror::Error;

/// H.264 preset names accepted by `libx264`, ordered fastest to slowest.
///
/// For VP9 the same names are mapped to `-cpu-used` from 8 down to 0. There are
/// ten names but nine `-cpu-used` values, so `veryslow` and `placebo` both map
/// to 0.
pub const PRESET_NAMES: [&str; 10] = [
    "ultrafast",
    "superfast",
    "veryfast",
    "faster",
    "fast",
    "medium",
    "slow",
    "slower",
    "veryslow",
    "placebo",
];

/// Slowest VP9 speed setting, used by the two slowest x264 preset names.
const MAX_CPU_USED: u8 = 8;

/// Accepted playback rates.
const MIN_FPS: u16 = 1;
const MAX_FPS: u16 = 60;

const MP4_DEFAULT_CRF: u8 = 23;
const MP4_MAX_CRF: u8 = 51;
const VP9_DEFAULT_CRF: u8 = 32;
const VP9_MAX_CRF: u8 = 63;

/// `yuv420p` needs even dimensions; padding is preferred over rejecting odd
/// canvases because ASCII cells often produce odd pixel sizes.
const EVEN_PAD_FILTER: &str = "pad=ceil(iw/2)*2:ceil(ih/2)*2";

/// Silent video container written by [`VideoEncoder`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoFormat {
    /// MP4 with H.264 (`libx264`).
    Mp4,
    /// WebM with VP9 (`libvpx-vp9`).
    WebM,
}

impl VideoFormat {
    /// CRF used when the caller does not override it.
    pub const fn default_crf(self) -> u8 {
        match self {
            Self::Mp4 => MP4_DEFAULT_CRF,
            Self::WebM => VP9_DEFAULT_CRF,
        }
    }

    /// Largest CRF accepted by the codec's valid range starting at zero.
    pub const fn max_crf(self) -> u8 {
        match self {
            Self::Mp4 => MP4_MAX_CRF,
            Self::WebM => VP9_MAX_CRF,
        }
    }

    const fn codec(self) -> &'static str {
        match self {
            Self::Mp4 => "libx264",
            Self::WebM => "libvpx-vp9",
        }
    }
}

impl fmt::Display for VideoFormat {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Mp4 => "mp4 (H.264)",
            Self::WebM => "webm (VP9)",
        })
    }
}

/// Options shared by the CLI and the library.
#[derive(Debug, Clone)]
pub struct VideoOptions {
    /// Frame width in pixels. Frames must match it exactly.
    pub width: u32,
    /// Frame height in pixels. Frames must match it exactly.
    pub height: u32,
    /// Playback rate, `1..=60` frames per second.
    pub fps: u16,
    /// Constant rate factor; `0..=51` for MP4 and `0..=63` for WebM.
    pub crf: u8,
    /// One of [`PRESET_NAMES`].
    pub preset: String,
    /// FFmpeg binary to spawn.
    pub ffmpeg_path: PathBuf,
}

/// Errors produced by the video exporter.
#[derive(Debug, Error)]
pub enum VideoError {
    #[error("failed to start ffmpeg at '{path}': {source}")]
    Spawn {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("invalid video options: {reason}")]
    InvalidOptions { reason: String },
    #[error(
        "frame is {actual_width}x{actual_height} but the video is \
         {expected_width}x{expected_height}"
    )]
    Dimensions {
        expected_width: u32,
        expected_height: u32,
        actual_width: u32,
        actual_height: u32,
    },
    #[error(
        "failed to stream video data to ffmpeg at '{path}' ({source}); check that \
         ffmpeg supports the requested encoder"
    )]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("ffmpeg exited with {status}: {stderr}")]
    Ffmpeg { status: ExitStatus, stderr: String },
}

/// Streaming video encoder backed by an FFmpeg child process.
///
/// Feed frames as they are rendered with [`VideoEncoder::write_frame`] and call
/// [`VideoEncoder::finish`] once the animation ends. Dropping an unfinished
/// encoder kills and reaps the child so no zombie FFmpeg process remains.
#[derive(Debug)]
pub struct VideoEncoder {
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    width: u32,
    height: u32,
    ffmpeg_path: PathBuf,
}

impl VideoEncoder {
    /// Spawn FFmpeg and configure it to read raw RGBA frames from stdin.
    ///
    /// # Errors
    ///
    /// Returns [`VideoError::InvalidOptions`] before spawning when dimensions,
    /// frame rate, CRF or preset are out of range, and [`VideoError::Spawn`]
    /// when the binary cannot be started (for example, because it is missing).
    pub fn new(
        path: &Path,
        format: VideoFormat,
        options: &VideoOptions,
    ) -> Result<Self, VideoError> {
        let args = ffmpeg_args(format, options)?;
        let mut child = Command::new(&options.ffmpeg_path)
            .args(args)
            .arg(path)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|source| VideoError::Spawn {
                path: options.ffmpeg_path.clone(),
                source,
            })?;
        let stdin = child
            .stdin
            .take()
            .expect("stdin is piped, so the child always exposes it");

        Ok(Self {
            child: Some(child),
            stdin: Some(stdin),
            width: options.width,
            height: options.height,
            ffmpeg_path: options.ffmpeg_path.clone(),
        })
    }

    /// Stream `image` to FFmpeg as the next raw RGBA frame.
    ///
    /// # Errors
    ///
    /// Returns [`VideoError::Dimensions`] before writing anything when the
    /// image size differs from the configured canvas, and [`VideoError::Io`]
    /// when FFmpeg's stdin is closed or the write fails.
    pub fn write_frame(&mut self, image: &RgbaImage) -> Result<(), VideoError> {
        if image.width() != self.width || image.height() != self.height {
            return Err(VideoError::Dimensions {
                expected_width: self.width,
                expected_height: self.height,
                actual_width: image.width(),
                actual_height: image.height(),
            });
        }

        let Some(stdin) = self.stdin.as_mut() else {
            return Err(self.io_error(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "ffmpeg stdin is already closed",
            )));
        };
        stdin
            .write_all(image.as_raw())
            .map_err(|source| self.io_error(source))
    }

    /// Close FFmpeg's stdin, wait for it to finish and report its exit status.
    ///
    /// # Errors
    ///
    /// Returns [`VideoError::Ffmpeg`] with the captured stderr when FFmpeg
    /// exits non-zero, and [`VideoError::Io`] when waiting for it fails.
    pub fn finish(mut self) -> Result<(), VideoError> {
        // Dropping the last stdin handle sends EOF: FFmpeg flushes the encoder
        // and finalizes the container only after this point.
        self.stdin = None;

        let Some(child) = self.child.take() else {
            return Ok(());
        };
        let output = child
            .wait_with_output()
            .map_err(|source| self.io_error(source))?;
        if output.status.success() {
            return Ok(());
        }

        let stderr = String::from_utf8_lossy(&output.stderr);
        let stderr = stderr.trim();
        let stderr = if stderr.is_empty() {
            "(ffmpeg wrote no stderr)".to_owned()
        } else {
            stderr.to_owned()
        };
        Err(VideoError::Ffmpeg {
            status: output.status,
            stderr,
        })
    }

    fn io_error(&self, source: std::io::Error) -> VideoError {
        VideoError::Io {
            path: self.ffmpeg_path.clone(),
            source,
        }
    }
}

impl Drop for VideoEncoder {
    fn drop(&mut self) {
        self.stdin = None;
        if let Some(mut child) = self.child.take() {
            // An unfinished encoder must not leave FFmpeg running or
            // unreaped; killing it also closes any pipe it still holds.
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// Build the FFmpeg argument list for a silent streamed video.
///
/// The returned list is complete except for the output path, which
/// [`VideoEncoder::new`] appends as the final argument. The input always reads
/// raw RGBA from stdin with an even-dimension pad filter.
///
/// # Errors
///
/// Propagates [`VideoError::InvalidOptions`] from option validation.
pub fn ffmpeg_args(format: VideoFormat, options: &VideoOptions) -> Result<Vec<String>, VideoError> {
    validate_options(format, options)?;

    let mut args = vec![
        "-hide_banner".to_owned(),
        "-loglevel".to_owned(),
        "error".to_owned(),
        "-nostdin".to_owned(),
        "-y".to_owned(),
        "-f".to_owned(),
        "rawvideo".to_owned(),
        "-pix_fmt".to_owned(),
        "rgba".to_owned(),
        "-s".to_owned(),
        format!("{}x{}", options.width, options.height),
        "-r".to_owned(),
        options.fps.to_string(),
        "-i".to_owned(),
        "-".to_owned(),
        "-vf".to_owned(),
        EVEN_PAD_FILTER.to_owned(),
        "-c:v".to_owned(),
        format.codec().to_owned(),
    ];

    match format {
        VideoFormat::Mp4 => args.extend([
            "-preset".to_owned(),
            options.preset.clone(),
            "-crf".to_owned(),
            options.crf.to_string(),
            "-pix_fmt".to_owned(),
            "yuv420p".to_owned(),
            "-movflags".to_owned(),
            "+faststart".to_owned(),
        ]),
        VideoFormat::WebM => {
            let cpu_used = preset_to_cpu_used(&options.preset)
                .expect("preset names are validated before arguments are built");
            args.extend([
                "-b:v".to_owned(),
                "0".to_owned(),
                "-crf".to_owned(),
                options.crf.to_string(),
                "-deadline".to_owned(),
                "good".to_owned(),
                "-cpu-used".to_owned(),
                cpu_used.to_string(),
                "-row-mt".to_owned(),
                "1".to_owned(),
                "-pix_fmt".to_owned(),
                "yuv420p".to_owned(),
            ]);
        }
    }

    Ok(args)
}

/// Map an x264 preset name to VP9's `-cpu-used`, from 8 (fastest) to 0.
///
/// The two slowest x264 names share the slowest VP9 setting:
/// `ultrafast` → 8, `superfast` → 7, `veryfast` → 6, `faster` → 5, `fast` → 4,
/// `medium` → 3, `slow` → 2, `slower` → 1, `veryslow` → 0 and `placebo` → 0.
/// Returns `None` for names outside [`PRESET_NAMES`].
pub fn preset_to_cpu_used(preset: &str) -> Option<u8> {
    PRESET_NAMES
        .iter()
        .position(|name| *name == preset)
        .map(|index| {
            let index = u8::try_from(index).unwrap_or(MAX_CPU_USED);
            MAX_CPU_USED.saturating_sub(index)
        })
}

fn validate_options(format: VideoFormat, options: &VideoOptions) -> Result<(), VideoError> {
    if options.width == 0 || options.height == 0 {
        return Err(VideoError::InvalidOptions {
            reason: format!(
                "width and height must be non-zero, got {}x{}",
                options.width, options.height
            ),
        });
    }
    if !(MIN_FPS..=MAX_FPS).contains(&options.fps) {
        return Err(VideoError::InvalidOptions {
            reason: format!("fps {} is out of range {MIN_FPS}..={MAX_FPS}", options.fps),
        });
    }
    if options.crf > format.max_crf() {
        return Err(VideoError::InvalidOptions {
            reason: format!(
                "crf {} is out of range 0..={} for {format}",
                options.crf,
                format.max_crf()
            ),
        });
    }
    if preset_to_cpu_used(&options.preset).is_none() {
        return Err(VideoError::InvalidOptions {
            reason: format!(
                "unknown preset '{}'; expected one of: {}",
                options.preset,
                PRESET_NAMES.join(", ")
            ),
        });
    }
    Ok(())
}
