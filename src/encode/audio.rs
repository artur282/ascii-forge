//! Audio muxing and voice-over synthesis.
//!
//! The exporter never decodes audio samples itself. It drives external
//! FFmpeg-family binaries: [`probe_duration`] reads a container duration with
//! `ffprobe`, [`mux_audio`] copies the video stream and encodes AAC with
//! `ffmpeg`, and [`generate_voice`] asks `espeak-ng` for a temporary WAV.
//! Commands are built as explicit argument arrays, so user text never reaches
//! a shell.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use thiserror::Error;

/// Default AAC bitrate for muxed audio, in kilobits per second.
const DEFAULT_BITRATE_KBPS: u32 = 192;

/// External binaries and encoding settings used by the audio exports.
#[derive(Debug, Clone)]
pub struct AudioOptions {
    /// FFmpeg binary that muxes the audio into the video.
    pub ffmpeg_path: PathBuf,
    /// ffprobe binary that reads the audio duration.
    pub ffprobe_path: PathBuf,
    /// AAC bitrate in kilobits per second.
    pub bitrate_kbps: u32,
}

impl Default for AudioOptions {
    fn default() -> Self {
        Self {
            ffmpeg_path: PathBuf::from("ffmpeg"),
            ffprobe_path: PathBuf::from("ffprobe"),
            bitrate_kbps: DEFAULT_BITRATE_KBPS,
        }
    }
}

/// Errors produced by the audio exports.
#[derive(Debug, Error)]
pub enum AudioError {
    #[error("`{tool}` was not found at {path}; install it or pass its path explicitly")]
    MissingTool {
        /// Tool the operation needs, such as `ffmpeg` or `espeak-ng`.
        tool: &'static str,
        /// Path that could not be spawned.
        path: PathBuf,
    },
    #[error("`{tool}` failed with {status}: {stderr}")]
    CommandFailed {
        /// Tool that exited unsuccessfully.
        tool: &'static str,
        /// Exit status reported by the operating system.
        status: String,
        /// Captured standard error, or the exit status when it was empty.
        stderr: String,
    },
    #[error("`ffprobe` could not read {path}: {detail}")]
    ProbeFailed {
        /// Media file ffprobe could not inspect.
        path: PathBuf,
        /// Captured standard error, or the exit status when it was empty.
        detail: String,
    },
    #[error("`ffprobe` returned an invalid duration for {path}: `{value}`")]
    InvalidDuration {
        /// Media file whose duration could not be parsed.
        path: PathBuf,
        /// Raw text returned by ffprobe.
        value: String,
    },
    #[error("invalid audio input: {reason}")]
    InvalidInput {
        /// What is wrong with the input and how to fix it.
        reason: String,
    },
    #[error("io error for {path}: {source}")]
    Io {
        /// Path whose metadata could not be read.
        path: PathBuf,
        /// Underlying filesystem error.
        source: std::io::Error,
    },
}

/// Read the container duration of `path` in seconds with `ffprobe`.
///
/// # Errors
///
/// Returns [`AudioError::InvalidInput`] when `path` is not a readable regular
/// file, [`AudioError::MissingTool`] when `ffprobe_path` cannot be spawned,
/// [`AudioError::ProbeFailed`] when ffprobe exits unsuccessfully and
/// [`AudioError::InvalidDuration`] when it does not print a finite positive
/// number.
pub fn probe_duration(path: &Path, ffprobe_path: &Path) -> Result<f64, AudioError> {
    require_file(path, "audio")?;

    let output = Command::new(ffprobe_path)
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
        ])
        .arg(path)
        .output()
        .map_err(|source| spawn_error("ffprobe", ffprobe_path, source))?;

    if !output.status.success() {
        return Err(AudioError::ProbeFailed {
            path: path.to_path_buf(),
            detail: error_detail(&output),
        });
    }

    parse_duration(path, &String::from_utf8_lossy(&output.stdout))
}

/// Mux `audio` into the MP4 at `video`, copying the video stream.
///
/// The audio is encoded as AAC and padded with silence so `-shortest` cuts the
/// output at the video duration. The video is never re-encoded.
///
/// # Errors
///
/// Returns [`AudioError::InvalidInput`] when an input is missing or the audio
/// duration is unusable, [`AudioError::MissingTool`] when a configured binary
/// cannot be spawned, and [`AudioError::CommandFailed`] with captured stderr
/// when FFmpeg exits unsuccessfully.
pub fn mux_audio(
    video: &Path,
    audio: &Path,
    output: &Path,
    options: &AudioOptions,
) -> Result<(), AudioError> {
    require_file(video, "video")?;
    // Probing validates the audio file and its duration in one step.
    probe_duration(audio, &options.ffprobe_path)?;

    let bitrate = format!("{}k", options.bitrate_kbps);
    let result = Command::new(&options.ffmpeg_path)
        .args(["-y", "-hide_banner", "-loglevel", "error", "-i"])
        .arg(video)
        .arg("-i")
        .arg(audio)
        .args([
            "-map", "0:v:0", "-map", "1:a:0", "-c:v", "copy", "-c:a", "aac", "-b:a",
        ])
        .arg(&bitrate)
        .args(["-af", "apad", "-shortest"])
        .arg(output)
        .output()
        .map_err(|source| spawn_error("ffmpeg", &options.ffmpeg_path, source))?;

    ensure_success("ffmpeg", &result)
}

/// Synthesize `text` into the WAV file at `wav` with `espeak-ng`.
///
/// The text is passed as a single argument after `--`, so it is never
/// interpreted as an option and never reaches a shell.
///
/// # Errors
///
/// Returns [`AudioError::InvalidInput`] when `text` is empty or whitespace
/// only, [`AudioError::MissingTool`] when `espeak_path` cannot be spawned, and
/// [`AudioError::CommandFailed`] with captured stderr when espeak-ng exits
/// unsuccessfully.
pub fn generate_voice(text: &str, wav: &Path, espeak_path: &Path) -> Result<(), AudioError> {
    if text.trim().is_empty() {
        return Err(AudioError::InvalidInput {
            reason: "voice text must contain at least one non-whitespace character".to_owned(),
        });
    }

    let result = Command::new(espeak_path)
        .arg("-w")
        .arg(wav)
        .arg("--")
        .arg(text)
        .output()
        .map_err(|source| spawn_error("espeak-ng", espeak_path, source))?;

    ensure_success("espeak-ng", &result)
}

/// Require `path` to be an existing regular file.
fn require_file(path: &Path, role: &str) -> Result<(), AudioError> {
    match std::fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => Ok(()),
        Ok(_) => Err(AudioError::InvalidInput {
            reason: format!("{role} path is not a regular file: {}", path.display()),
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Err(AudioError::InvalidInput {
                reason: format!("{role} file does not exist: {}", path.display()),
            })
        }
        Err(source) => Err(AudioError::Io {
            path: path.to_path_buf(),
            source,
        }),
    }
}

fn parse_duration(path: &Path, raw: &str) -> Result<f64, AudioError> {
    let value = raw.trim();
    let duration = value
        .parse::<f64>()
        .map_err(|_| invalid_duration(path, value))?;

    if !duration.is_finite() || duration <= 0.0 {
        return Err(invalid_duration(path, value));
    }
    Ok(duration)
}

fn invalid_duration(path: &Path, value: &str) -> AudioError {
    AudioError::InvalidDuration {
        path: path.to_path_buf(),
        value: value.to_owned(),
    }
}

fn spawn_error(tool: &'static str, path: &Path, source: std::io::Error) -> AudioError {
    if source.kind() == std::io::ErrorKind::NotFound {
        AudioError::MissingTool {
            tool,
            path: path.to_path_buf(),
        }
    } else {
        AudioError::Io {
            path: path.to_path_buf(),
            source,
        }
    }
}

fn ensure_success(tool: &'static str, output: &Output) -> Result<(), AudioError> {
    if output.status.success() {
        return Ok(());
    }
    Err(AudioError::CommandFailed {
        tool,
        status: output.status.to_string(),
        stderr: error_detail(output),
    })
}

/// Captured stderr, falling back to the exit status when the tool was silent.
fn error_detail(output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    if stderr.is_empty() {
        output.status.to_string()
    } else {
        stderr
    }
}
