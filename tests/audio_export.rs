//! Integration tests for the audio muxing and voice-over module (issue #11,
//! specs S3 and S5).
//!
//! The module is exercised through its public API only. Fixtures are real
//! media files generated with FFmpeg, and assertions go through `ffprobe`, so
//! tests that need a binary skip cleanly when it is absent. Validation and
//! argument-error tests run without any external tool.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use ascii_forge::encode::audio::{
    AudioError, AudioOptions, generate_voice, mux_audio, probe_duration,
};
use serde_json::Value;

/// Frame size of the generated video fixture.
const VIDEO_SIZE: u32 = 64;

/// Seconds of the generated silent video fixture.
const VIDEO_SECONDS: f64 = 1.0;

fn tool_available(binary: &str, version_flag: &str) -> bool {
    Command::new(binary)
        .arg(version_flag)
        .output()
        .is_ok_and(|output| output.status.success())
}

fn ffmpeg_available() -> bool {
    tool_available("ffmpeg", "-version")
}

fn ffprobe_available() -> bool {
    tool_available("ffprobe", "-version")
}

fn espeak_available() -> bool {
    tool_available("espeak-ng", "--version")
}

/// Generate a one-second 64x64 black H.264 MP4.
fn make_silent_video(path: &Path) {
    let status = Command::new("ffmpeg")
        .args([
            "-y",
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=c=black:s=64x64:r=30:d=1",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
        ])
        .arg(path)
        .status()
        .expect("run ffmpeg to create the video fixture");
    assert!(status.success(), "failed to create {}", path.display());
}

/// Generate a one-second 440 Hz WAV.
fn make_tone(path: &Path) {
    make_tone_with_duration(path, "1");
}

/// Generate a 440 Hz WAV of `duration` seconds.
fn make_tone_with_duration(path: &Path, duration: &str) {
    let source = format!("sine=frequency=440:duration={duration}");
    let status = Command::new("ffmpeg")
        .args([
            "-y",
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
        ])
        .arg(source)
        .arg(path)
        .status()
        .expect("run ffmpeg to create the tone fixture");
    assert!(status.success(), "failed to create {}", path.display());
}

fn probe_streams(path: &Path) -> Value {
    let output = Command::new("ffprobe")
        .args(["-v", "error", "-print_format", "json", "-show_streams"])
        .arg(path)
        .output()
        .expect("run ffprobe");
    assert!(
        output.status.success(),
        "ffprobe failed for {}: {}",
        path.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("parse ffprobe stream JSON")
}

fn probe_format_duration(path: &Path) -> f64 {
    let output = Command::new("ffprobe")
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
        .expect("run ffprobe");
    assert!(
        output.status.success(),
        "ffprobe failed for {}: {}",
        path.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse()
        .expect("parse ffprobe duration")
}

fn streams_of_codec_type<'a>(streams: &'a Value, codec_type: &str) -> Vec<&'a Value> {
    streams["streams"]
        .as_array()
        .expect("ffprobe streams array")
        .iter()
        .filter(|stream| stream["codec_type"].as_str() == Some(codec_type))
        .collect()
}

#[test]
fn default_options_match_the_documented_contract() {
    let options = AudioOptions::default();

    assert_eq!(options.ffmpeg_path, PathBuf::from("ffmpeg"));
    assert_eq!(options.ffprobe_path, PathBuf::from("ffprobe"));
    assert_eq!(options.bitrate_kbps, 192);

    let cloned = options.clone();
    assert_eq!(cloned.bitrate_kbps, options.bitrate_kbps);
}

#[test]
fn probe_duration_reads_a_generated_wav() {
    if !ffmpeg_available() || !ffprobe_available() {
        return;
    }
    let dir = tempfile::tempdir().expect("temp dir");
    let wav = dir.path().join("tone.wav");
    make_tone(&wav);

    let duration = probe_duration(&wav, Path::new("ffprobe")).expect("probe duration");

    assert!(
        (duration - VIDEO_SECONDS).abs() < 0.1,
        "expected about one second, got {duration}"
    );
}

#[test]
fn probe_duration_rejects_a_missing_file() {
    let dir = tempfile::tempdir().expect("temp dir");
    let missing = dir.path().join("missing.wav");

    let error = probe_duration(&missing, Path::new("/nonexistent/ffprobe"))
        .expect_err("a missing file must be rejected");

    assert!(
        matches!(error, AudioError::InvalidInput { .. }),
        "unexpected error: {error}"
    );
    assert!(
        error.to_string().contains("missing.wav"),
        "message must name the file: {error}"
    );
}

#[test]
fn probe_duration_rejects_an_invalid_audio_file() {
    if !ffprobe_available() {
        return;
    }
    let dir = tempfile::tempdir().expect("temp dir");
    let bogus = dir.path().join("bogus.wav");
    fs::write(&bogus, b"this is not an audio file").expect("write bogus file");

    let error = probe_duration(&bogus, Path::new("ffprobe"))
        .expect_err("an invalid container must be rejected");

    assert!(
        matches!(
            error,
            AudioError::ProbeFailed { .. } | AudioError::InvalidDuration { .. }
        ),
        "unexpected error: {error}"
    );
    assert!(
        error.to_string().contains("bogus.wav"),
        "message must name the file: {error}"
    );
}

#[test]
fn probe_duration_names_a_missing_ffprobe_binary() {
    let dir = tempfile::tempdir().expect("temp dir");
    let wav = dir.path().join("placeholder.wav");
    fs::write(&wav, b"placeholder").expect("write placeholder");

    let error = probe_duration(&wav, Path::new("/nonexistent/ffprobe-binary"))
        .expect_err("a missing ffprobe must be rejected");

    assert!(
        matches!(error, AudioError::MissingTool { .. }),
        "unexpected error: {error}"
    );
    let message = error.to_string();
    assert!(
        message.contains("ffprobe"),
        "message must name the tool: {message}"
    );
    assert!(
        message.contains("ffprobe-binary"),
        "message must name the path: {message}"
    );
}

#[test]
fn mux_audio_rejects_missing_input_files() {
    let dir = tempfile::tempdir().expect("temp dir");
    let video = dir.path().join("missing.mp4");
    let audio = dir.path().join("missing.wav");
    let output = dir.path().join("output.mp4");

    let error = mux_audio(&video, &audio, &output, &AudioOptions::default())
        .expect_err("a missing video must be rejected");
    assert!(
        matches!(error, AudioError::InvalidInput { .. }),
        "unexpected error: {error}"
    );
    assert!(error.to_string().contains("missing.mp4"));

    fs::write(&video, b"placeholder").expect("write placeholder video");
    let error = mux_audio(&video, &audio, &output, &AudioOptions::default())
        .expect_err("a missing audio file must be rejected");
    assert!(
        matches!(error, AudioError::InvalidInput { .. }),
        "unexpected error: {error}"
    );
    assert!(error.to_string().contains("missing.wav"));
    assert!(!output.exists(), "validation must not create the output");
}

#[test]
fn mux_audio_names_a_missing_ffmpeg_binary() {
    if !ffmpeg_available() || !ffprobe_available() {
        return;
    }
    let dir = tempfile::tempdir().expect("temp dir");
    let video = dir.path().join("silent.mp4");
    let tone = dir.path().join("tone.wav");
    let output = dir.path().join("output.mp4");
    make_silent_video(&video);
    make_tone(&tone);
    let options = AudioOptions {
        ffmpeg_path: PathBuf::from("/nonexistent/ffmpeg-binary"),
        ..AudioOptions::default()
    };

    let error =
        mux_audio(&video, &tone, &output, &options).expect_err("a missing ffmpeg must be rejected");

    assert!(
        matches!(error, AudioError::MissingTool { .. }),
        "unexpected error: {error}"
    );
    let message = error.to_string();
    assert!(
        message.contains("ffmpeg"),
        "message must name the tool: {message}"
    );
    assert!(
        message.contains("ffmpeg-binary"),
        "message must name the path: {message}"
    );
    assert!(!output.exists(), "a failed mux must not create the output");
}

#[test]
fn mux_audio_adds_aac_audio_and_keeps_the_h264_video() {
    if !ffmpeg_available() || !ffprobe_available() {
        return;
    }
    let dir = tempfile::tempdir().expect("temp dir");
    let video = dir.path().join("silent.mp4");
    let tone = dir.path().join("tone.wav");
    let output = dir.path().join("muxed.mp4");
    make_silent_video(&video);
    make_tone(&tone);

    mux_audio(&video, &tone, &output, &AudioOptions::default()).expect("mux audio");

    assert!(output.is_file(), "the muxed file must exist");
    let streams = probe_streams(&output);
    let video_streams = streams_of_codec_type(&streams, "video");
    let audio_streams = streams_of_codec_type(&streams, "audio");
    assert_eq!(video_streams.len(), 1, "expected one video stream");
    assert_eq!(audio_streams.len(), 1, "expected one audio stream");

    let video_stream = video_streams[0];
    assert_eq!(video_stream["codec_name"].as_str(), Some("h264"));
    assert_eq!(video_stream["width"].as_u64(), Some(u64::from(VIDEO_SIZE)));
    assert_eq!(video_stream["height"].as_u64(), Some(u64::from(VIDEO_SIZE)));

    let audio_stream = audio_streams[0];
    assert_eq!(audio_stream["codec_name"].as_str(), Some("aac"));

    let duration = probe_format_duration(&output);
    assert!(
        (duration - VIDEO_SECONDS).abs() < 0.2,
        "expected the video duration, got {duration}"
    );
}

#[test]
fn mux_audio_aligns_the_output_duration_to_the_video() {
    if !ffmpeg_available() || !ffprobe_available() {
        return;
    }
    let dir = tempfile::tempdir().expect("temp dir");
    let video = dir.path().join("silent.mp4");
    make_silent_video(&video);

    let long_tone = dir.path().join("long.wav");
    let cut = dir.path().join("cut.mp4");
    make_tone_with_duration(&long_tone, "2");
    mux_audio(&video, &long_tone, &cut, &AudioOptions::default()).expect("mux long audio");
    let cut_duration = probe_format_duration(&cut);
    assert!(
        cut_duration < 1.5,
        "a 2s tone must be cut to the 1s video, got {cut_duration}"
    );
    assert!(
        (cut_duration - VIDEO_SECONDS).abs() < 0.2,
        "expected the video duration, got {cut_duration}"
    );

    let short_tone = dir.path().join("short.wav");
    let padded = dir.path().join("padded.mp4");
    make_tone_with_duration(&short_tone, "0.25");
    mux_audio(&video, &short_tone, &padded, &AudioOptions::default()).expect("mux short audio");
    let padded_duration = probe_format_duration(&padded);
    assert!(
        (padded_duration - VIDEO_SECONDS).abs() < 0.2,
        "silence must pad the 0.25s tone to the video duration, got {padded_duration}"
    );
}

#[test]
fn generate_voice_writes_a_readable_wav() {
    if !espeak_available() || !ffprobe_available() {
        return;
    }
    let dir = tempfile::tempdir().expect("temp dir");
    let wav = dir.path().join("voice.wav");

    generate_voice("Hello from ASCII Forge", &wav, Path::new("espeak-ng"))
        .expect("synthesize voice");

    assert!(wav.is_file(), "the voice WAV must exist");
    let bytes = fs::metadata(&wav).expect("voice metadata").len();
    assert!(
        bytes > 44,
        "a WAV file needs more than its header, got {bytes}"
    );
    let duration = probe_duration(&wav, Path::new("ffprobe")).expect("probe voice duration");
    assert!(duration > 0.2, "expected audible speech, got {duration}s");
    let streams = probe_streams(&wav);
    assert_eq!(
        streams_of_codec_type(&streams, "audio").len(),
        1,
        "the voice WAV must contain one audio stream"
    );
}

#[test]
fn generate_voice_names_a_missing_espeak_binary() {
    let dir = tempfile::tempdir().expect("temp dir");
    let wav = dir.path().join("voice.wav");

    let error = generate_voice("hello", &wav, Path::new("/nonexistent/espeak-ng-binary"))
        .expect_err("a missing espeak-ng must be rejected");

    assert!(
        matches!(error, AudioError::MissingTool { .. }),
        "unexpected error: {error}"
    );
    let message = error.to_string();
    assert!(
        message.contains("espeak-ng"),
        "message must name the tool: {message}"
    );
    assert!(
        message.contains("espeak-ng-binary"),
        "message must name the path: {message}"
    );
    assert!(!wav.exists(), "a failed synthesis must not create the WAV");
}

#[test]
fn generate_voice_rejects_empty_text() {
    let dir = tempfile::tempdir().expect("temp dir");

    for (index, text) in ["", "   ", "\n\t"].into_iter().enumerate() {
        let wav = dir.path().join(format!("empty-{index}.wav"));
        let error = generate_voice(text, &wav, Path::new("/nonexistent/espeak-ng"))
            .expect_err("empty text must be rejected");

        assert!(
            matches!(error, AudioError::InvalidInput { .. }),
            "case {index} produced {error}"
        );
        assert!(!wav.exists(), "case {index} must not create a WAV");
    }
}
