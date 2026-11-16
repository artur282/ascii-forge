//! Video and GIF decoding through FFmpeg into RGBA frames.

use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use image::RgbaImage;
use serde::Deserialize;

use super::{Grid, ImportError, ImportOptions, rgba_image_to_grid, rows_for};

/// Decoded video frames plus source metadata.
#[derive(Debug, Clone)]
pub struct VideoImport {
    pub frames: Vec<Grid>,
    pub fps: u16,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Deserialize)]
struct Probe {
    streams: Vec<Stream>,
}

#[derive(Debug, Deserialize)]
struct Stream {
    width: u32,
    height: u32,
}

/// Decode a video or GIF through FFmpeg and convert its frames to ASCII.
pub fn import_video(
    path: &Path,
    options: &ImportOptions,
    ffmpeg: &Path,
    ffprobe: &Path,
) -> Result<VideoImport, ImportError> {
    options.validate()?;
    let probe = probe_video(path, ffprobe)?;
    let rows = options
        .rows
        .unwrap_or_else(|| rows_for(options.cols, probe.width, probe.height, options.cell_aspect));
    let (scale_width, scale_height) = if options.braille {
        (u32::from(options.cols) * 2, u32::from(rows) * 4)
    } else {
        (u32::from(options.cols), u32::from(rows))
    };

    let filter = format!(
        "fps={},scale={scale_width}:{scale_height}:flags=area",
        options.fps
    );
    let mut child = Command::new(ffmpeg)
        .arg("-v")
        .arg("error")
        .arg("-i")
        .arg(path)
        .arg("-an")
        .arg("-vf")
        .arg(&filter)
        .arg("-frames:v")
        .arg(options.max_frames.to_string())
        .arg("-f")
        .arg("rawvideo")
        .arg("-pix_fmt")
        .arg("rgba")
        .arg("-")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| ImportError::Video(format!("could not start ffmpeg: {error}")))?;

    let stderr = child.stderr.take().map(|mut stderr| {
        std::thread::spawn(move || {
            let mut message = String::new();
            let _ = stderr.read_to_string(&mut message);
            message
        })
    });
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| ImportError::Video("ffmpeg stdout is unavailable".to_string()))?;
    let mut reader = io::BufReader::new(stdout);
    let frame_len = scale_width as usize * scale_height as usize * 4;
    let mut buffer = vec![0_u8; frame_len];
    let mut frames = Vec::new();
    let frame_options = ImportOptions {
        rows: Some(rows),
        ..options.clone()
    };

    loop {
        match read_frame(&mut reader, &mut buffer) {
            Ok(true) => {
                let image = RgbaImage::from_raw(scale_width, scale_height, buffer.clone())
                    .ok_or_else(|| {
                        ImportError::Video("ffmpeg returned an invalid frame size".to_string())
                    })?;
                frames.push(rgba_image_to_grid(image, &frame_options)?);
            }
            Ok(false) => break,
            Err(error) => {
                let _ = child.kill();
                let message = stderr
                    .and_then(|handle| handle.join().ok())
                    .unwrap_or_default();
                return Err(ImportError::Video(format!(
                    "could not read ffmpeg frame: {error}; {message}"
                )));
            }
        }
    }

    let status = child
        .wait()
        .map_err(|error| ImportError::Video(format!("could not wait for ffmpeg: {error}")))?;
    let message = stderr
        .and_then(|handle| handle.join().ok())
        .unwrap_or_default();
    if !status.success() {
        return Err(ImportError::Video(format!(
            "ffmpeg exited with {status}: {}",
            message.trim()
        )));
    }
    if frames.is_empty() {
        return Err(ImportError::Video(
            "ffmpeg produced no video frames".to_string(),
        ));
    }

    Ok(VideoImport {
        frames,
        fps: options.fps,
        width: probe.width,
        height: probe.height,
    })
}

fn probe_video(path: &Path, ffprobe: &Path) -> Result<Stream, ImportError> {
    let output = Command::new(ffprobe)
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height",
            "-of",
            "json",
        ])
        .arg(path)
        .output()
        .map_err(|error| ImportError::Video(format!("could not start ffprobe: {error}")))?;
    if !output.status.success() {
        return Err(ImportError::Video(format!(
            "ffprobe exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    let probe: Probe = serde_json::from_slice(&output.stdout)
        .map_err(|error| ImportError::Video(format!("invalid ffprobe JSON: {error}")))?;
    let stream = probe
        .streams
        .first()
        .ok_or_else(|| ImportError::Video("input has no video stream".to_string()))?;
    if stream.width == 0 || stream.height == 0 {
        return Err(ImportError::Video(
            "input has zero-sized video frames".to_string(),
        ));
    }
    Ok(Stream {
        width: stream.width,
        height: stream.height,
    })
}

fn read_frame(reader: &mut impl Read, buffer: &mut [u8]) -> io::Result<bool> {
    let mut filled = 0;
    while filled < buffer.len() {
        let count = reader.read(&mut buffer[filled..])?;
        if count == 0 {
            if filled == 0 {
                return Ok(false);
            }
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "partial video frame",
            ));
        }
        filled += count;
    }
    Ok(true)
}

/// Resolve `ffprobe` next to a custom `ffmpeg` path when possible.
pub fn default_ffprobe(ffmpeg: &Path) -> PathBuf {
    ffmpeg
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(|parent| parent.join("ffprobe"))
        .filter(|candidate| candidate.is_file())
        .unwrap_or_else(|| PathBuf::from("ffprobe"))
}
