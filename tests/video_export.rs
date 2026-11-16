//! Integration tests for the streaming FFmpeg video exporter (issue #10, S1/S2).
//!
//! `VideoEncoder` is exercised through its public API only: frames are written
//! one at a time to a real FFmpeg process and the resulting file is inspected
//! with `ffprobe`. No assertion depends on encoder versions or file sizes.
//!
//! Option validation, argument construction, preset mapping and spawn failures
//! are pure checks that always run. Tests that need a real encoder skip cleanly
//! when `ffmpeg` or `ffprobe` is absent.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use ascii_forge::encode::video::{
    PRESET_NAMES, VideoEncoder, VideoError, VideoFormat, VideoOptions, ffmpeg_args,
    preset_to_cpu_used,
};
use image::RgbaImage;
use serde_json::Value;

/// Run `<tool> -version` and report whether it succeeded.
fn tool_available(tool: &str) -> bool {
    Command::new(tool)
        .arg("-version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn ffmpeg_available() -> bool {
    tool_available("ffmpeg")
}

fn ffprobe_available() -> bool {
    tool_available("ffprobe")
}

/// Skip integration tests cleanly when FFmpeg or ffprobe is missing.
fn skip_unless_ffmpeg() -> bool {
    if ffmpeg_available() && ffprobe_available() {
        return false;
    }
    eprintln!("skipping: ffmpeg and ffprobe are required");
    true
}

/// Whether `ffmpeg -encoders` lists `name`.
fn encoder_available(name: &str) -> bool {
    let Ok(output) = Command::new("ffmpeg")
        .args(["-hide_banner", "-encoders"])
        .stdin(Stdio::null())
        .output()
    else {
        return false;
    };
    output.status.success() && String::from_utf8_lossy(&output.stdout).contains(name)
}

fn probe_json(path: &Path) -> Value {
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
            "-count_frames",
        ])
        .arg(path)
        .output()
        .expect("run ffprobe");
    assert!(
        output.status.success(),
        "ffprobe failed on {}: {}",
        path.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("parse ffprobe json")
}

fn video_stream(probe: &Value) -> &Value {
    probe["streams"]
        .as_array()
        .and_then(|streams| {
            streams
                .iter()
                .find(|stream| stream["codec_type"] == "video")
        })
        .expect("probe json has a video stream")
}

/// Decoded frame count from `-count_frames`, with `nb_frames` as a fallback.
fn frame_count(probe: &Value) -> u32 {
    let stream = video_stream(probe);
    for key in ["nb_read_frames", "nb_frames"] {
        match stream.get(key) {
            Some(Value::String(text)) => {
                if let Ok(count) = text.parse() {
                    return count;
                }
            }
            Some(Value::Number(number)) => {
                if let Some(count) = number.as_u64() {
                    return u32::try_from(count).expect("frame count fits in u32");
                }
            }
            _ => {}
        }
    }
    panic!("no frame count in stream: {stream}");
}

fn assert_rate(stream: &Value, expected: &str) {
    let actual = ["avg_frame_rate", "r_frame_rate"]
        .iter()
        .find_map(|key| stream.get(*key).and_then(Value::as_str));
    assert_eq!(actual, Some(expected), "unexpected frame rate: {actual:?}");
}

fn assert_playable(path: &Path) {
    let output = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(path)
        .args(["-f", "null", "-"])
        .stdin(Stdio::null())
        .output()
        .expect("run ffmpeg decode");
    assert!(
        output.status.success(),
        "{} is not decodable: {}",
        path.display(),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// The value that follows `flag` in a constructed argument list.
fn arg_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.windows(2)
        .find(|pair| pair[0] == flag)
        .map(|pair| pair[1].as_str())
}

/// Deterministic colored pattern so every frame differs.
fn sample_frame(width: u32, height: u32, step: u8) -> RgbaImage {
    let mut image = RgbaImage::new(width, height);
    for (x, y, pixel) in image.enumerate_pixels_mut() {
        *pixel = image::Rgba([
            40_u8
                .wrapping_add(step.wrapping_mul(20))
                .wrapping_add((x % 64) as u8),
            90_u8.wrapping_add((y % 64) as u8),
            200_u8.wrapping_sub(step.wrapping_mul(10)),
            255,
        ]);
    }
    image
}

fn options_for(format: VideoFormat, width: u32, height: u32) -> VideoOptions {
    VideoOptions {
        width,
        height,
        fps: 30,
        crf: format.default_crf(),
        preset: "ultrafast".to_owned(),
        ffmpeg_path: PathBuf::from("ffmpeg"),
    }
}

/// Options whose FFmpeg path does not exist, so only validation can succeed.
fn unspawnable_options(format: VideoFormat) -> VideoOptions {
    let mut options = options_for(format, 16, 16);
    options.ffmpeg_path = PathBuf::from("/definitely/not/ffmpeg");
    options
}

/// Start an encoder, retrying the transient `ETXTBSY` of a just-written script.
fn start_fake_ffmpeg(path: &Path, format: VideoFormat, options: &VideoOptions) -> VideoEncoder {
    for _ in 0..20 {
        match VideoEncoder::new(path, format, options) {
            Ok(encoder) => return encoder,
            Err(VideoError::Spawn { source, .. })
                if source.kind() == std::io::ErrorKind::ExecutableFileBusy =>
            {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            Err(error) => panic!("spawn fake ffmpeg: {error}"),
        }
    }
    panic!("fake ffmpeg stayed busy after 20 attempts");
}

#[test]
fn documented_crf_defaults_match_the_specs() {
    assert_eq!(VideoFormat::Mp4.default_crf(), 23);
    assert_eq!(VideoFormat::WebM.default_crf(), 32);
    assert_eq!(VideoFormat::Mp4.max_crf(), 51);
    assert_eq!(VideoFormat::WebM.max_crf(), 63);
}

#[test]
fn x264_presets_map_to_vp9_cpu_used() {
    let expected = [8, 7, 6, 5, 4, 3, 2, 1, 0, 0];
    assert_eq!(
        PRESET_NAMES.len(),
        expected.len(),
        "the preset list must document the full mapping"
    );

    for (preset, cpu_used) in PRESET_NAMES.iter().zip(expected) {
        assert_eq!(
            preset_to_cpu_used(preset),
            Some(cpu_used),
            "unexpected mapping for {preset}"
        );
    }
    assert_eq!(preset_to_cpu_used("turbo"), None);
}

#[test]
fn mp4_arguments_request_h264_with_faststart_and_padding() {
    let options = options_for(VideoFormat::Mp4, 63, 63);
    let args = ffmpeg_args(VideoFormat::Mp4, &options).expect("valid mp4 args");

    assert_eq!(arg_value(&args, "-f"), Some("rawvideo"));
    assert_eq!(arg_value(&args, "-pix_fmt"), Some("rgba"));
    assert_eq!(arg_value(&args, "-s"), Some("63x63"));
    assert_eq!(arg_value(&args, "-r"), Some("30"));
    assert_eq!(arg_value(&args, "-i"), Some("-"));
    assert_eq!(
        arg_value(&args, "-vf"),
        Some("pad=ceil(iw/2)*2:ceil(ih/2)*2")
    );
    assert_eq!(arg_value(&args, "-c:v"), Some("libx264"));
    assert_eq!(arg_value(&args, "-preset"), Some("ultrafast"));
    assert_eq!(arg_value(&args, "-crf"), Some("23"));
    assert_eq!(arg_value(&args, "-movflags"), Some("+faststart"));
    assert!(
        args.windows(2)
            .any(|pair| pair[0] == "-pix_fmt" && pair[1] == "yuv420p"),
        "mp4 output must convert to yuv420p: {args:?}"
    );
    assert!(
        args.iter().any(|arg| arg == "-y"),
        "ffmpeg must overwrite the output: {args:?}"
    );
}

#[test]
fn webm_arguments_request_vp9_with_the_preset_mapped_to_cpu_used() {
    let mut options = options_for(VideoFormat::WebM, 64, 64);
    options.preset = "slow".to_owned();
    let args = ffmpeg_args(VideoFormat::WebM, &options).expect("valid webm args");

    assert_eq!(arg_value(&args, "-f"), Some("rawvideo"));
    assert_eq!(arg_value(&args, "-c:v"), Some("libvpx-vp9"));
    assert_eq!(arg_value(&args, "-b:v"), Some("0"));
    assert_eq!(arg_value(&args, "-crf"), Some("32"));
    assert_eq!(arg_value(&args, "-deadline"), Some("good"));
    assert_eq!(arg_value(&args, "-cpu-used"), Some("2"));
    assert_eq!(arg_value(&args, "-row-mt"), Some("1"));
    assert!(
        args.windows(2)
            .any(|pair| pair[0] == "-pix_fmt" && pair[1] == "yuv420p"),
        "webm output must convert to yuv420p: {args:?}"
    );
    assert_eq!(arg_value(&args, "-preset"), None);
    assert_eq!(arg_value(&args, "-movflags"), None);
}

#[test]
fn invalid_options_are_rejected_before_spawning() {
    let dir = tempfile::tempdir().expect("temp dir");
    let cases = [
        (
            "fps zero",
            VideoFormat::Mp4,
            VideoOptions {
                fps: 0,
                ..unspawnable_options(VideoFormat::Mp4)
            },
        ),
        (
            "fps above sixty",
            VideoFormat::Mp4,
            VideoOptions {
                fps: 61,
                ..unspawnable_options(VideoFormat::Mp4)
            },
        ),
        (
            "zero width",
            VideoFormat::Mp4,
            VideoOptions {
                width: 0,
                ..unspawnable_options(VideoFormat::Mp4)
            },
        ),
        (
            "zero height",
            VideoFormat::Mp4,
            VideoOptions {
                height: 0,
                ..unspawnable_options(VideoFormat::Mp4)
            },
        ),
        (
            "mp4 crf above 51",
            VideoFormat::Mp4,
            VideoOptions {
                crf: 52,
                ..unspawnable_options(VideoFormat::Mp4)
            },
        ),
        (
            "webm crf above 63",
            VideoFormat::WebM,
            VideoOptions {
                crf: 64,
                ..unspawnable_options(VideoFormat::WebM)
            },
        ),
        (
            "unknown preset",
            VideoFormat::Mp4,
            VideoOptions {
                preset: "turbo".to_owned(),
                ..unspawnable_options(VideoFormat::Mp4)
            },
        ),
    ];

    for (index, (label, format, options)) in cases.into_iter().enumerate() {
        let path = dir.path().join(format!("invalid-{index}.mp4"));
        let error = VideoEncoder::new(&path, format, &options)
            .expect_err(&format!("{label} must be rejected"));
        assert!(
            matches!(error, VideoError::InvalidOptions { .. }),
            "{label}: {error}"
        );
        assert!(!path.exists(), "{label} must not create the output file");
    }
}

#[test]
fn missing_ffmpeg_error_names_the_binary_and_path() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("missing.mp4");
    let options = unspawnable_options(VideoFormat::Mp4);

    let error = VideoEncoder::new(&path, VideoFormat::Mp4, &options)
        .expect_err("a missing ffmpeg must fail");

    assert!(matches!(error, VideoError::Spawn { .. }), "{error}");
    let message = error.to_string();
    assert!(message.contains("ffmpeg"), "{message}");
    assert!(message.contains("/definitely/not/ffmpeg"), "{message}");
}

#[test]
fn mp4_streams_six_h264_frames() {
    if skip_unless_ffmpeg() {
        return;
    }
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("six.mp4");
    let options = options_for(VideoFormat::Mp4, 64, 64);

    let mut encoder = VideoEncoder::new(&path, VideoFormat::Mp4, &options).expect("start ffmpeg");
    for step in 0..6 {
        encoder
            .write_frame(&sample_frame(64, 64, step))
            .expect("write frame");
    }
    encoder.finish().expect("finish mp4");

    let probe = probe_json(&path);
    let stream = video_stream(&probe);
    assert_eq!(stream["codec_name"], "h264");
    assert_eq!(stream["width"].as_u64(), Some(64));
    assert_eq!(stream["height"].as_u64(), Some(64));
    assert_eq!(frame_count(&probe), 6);
    assert_rate(stream, "30/1");
}

#[test]
fn webm_streams_three_vp9_frames() {
    if skip_unless_ffmpeg() {
        return;
    }
    if !encoder_available("libvpx-vp9") {
        eprintln!("skipping: ffmpeg has no libvpx-vp9 encoder");
        return;
    }
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("three.webm");
    let options = options_for(VideoFormat::WebM, 64, 64);

    let mut encoder = VideoEncoder::new(&path, VideoFormat::WebM, &options).expect("start ffmpeg");
    for step in 0..3 {
        encoder
            .write_frame(&sample_frame(64, 64, step))
            .expect("write frame");
    }
    encoder.finish().expect("finish webm");

    let probe = probe_json(&path);
    let stream = video_stream(&probe);
    assert_eq!(stream["codec_name"], "vp9");
    assert_eq!(stream["width"].as_u64(), Some(64));
    assert_eq!(stream["height"].as_u64(), Some(64));
    assert_eq!(frame_count(&probe), 3);
    assert_rate(stream, "30/1");
}

#[test]
fn odd_dimensions_are_padded_to_even_size() {
    if skip_unless_ffmpeg() {
        return;
    }
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("odd.mp4");
    let options = options_for(VideoFormat::Mp4, 63, 63);

    let mut encoder = VideoEncoder::new(&path, VideoFormat::Mp4, &options).expect("start ffmpeg");
    for step in 0..2 {
        encoder
            .write_frame(&sample_frame(63, 63, step))
            .expect("write frame");
    }
    encoder.finish().expect("finish odd-sized mp4");

    let probe = probe_json(&path);
    let stream = video_stream(&probe);
    assert_eq!(stream["width"].as_u64(), Some(64));
    assert_eq!(stream["height"].as_u64(), Some(64));
    assert_playable(&path);
}

#[test]
fn mismatched_frame_is_rejected_without_writing() {
    if skip_unless_ffmpeg() {
        return;
    }
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("mismatch.mp4");
    let options = options_for(VideoFormat::Mp4, 64, 64);

    let mut encoder = VideoEncoder::new(&path, VideoFormat::Mp4, &options).expect("start ffmpeg");
    encoder
        .write_frame(&sample_frame(64, 64, 0))
        .expect("write first frame");

    let error = encoder
        .write_frame(&RgbaImage::new(32, 32))
        .expect_err("a mismatched frame must be rejected");
    assert!(matches!(error, VideoError::Dimensions { .. }), "{error}");
    let message = error.to_string();
    assert!(message.contains("64x64"), "{message}");
    assert!(message.contains("32x32"), "{message}");

    for step in 1..6 {
        encoder
            .write_frame(&sample_frame(64, 64, step))
            .expect("encoder must stay usable after a rejected frame");
    }
    encoder.finish().expect("finish");

    assert_eq!(
        frame_count(&probe_json(&path)),
        6,
        "the rejected frame must not reach ffmpeg"
    );
}

#[cfg(unix)]
#[test]
fn non_zero_exit_reports_the_captured_stderr() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().expect("temp dir");
    let ffmpeg = dir.path().join("fake-ffmpeg");
    fs::write(
        &ffmpeg,
        "#!/bin/sh\necho 'boom: encoder exploded' >&2\nexit 3\n",
    )
    .expect("write fake ffmpeg");
    fs::set_permissions(&ffmpeg, fs::Permissions::from_mode(0o755)).expect("chmod fake ffmpeg");

    let path = dir.path().join("out.mp4");
    let mut options = options_for(VideoFormat::Mp4, 16, 16);
    options.ffmpeg_path = ffmpeg;

    let encoder = start_fake_ffmpeg(&path, VideoFormat::Mp4, &options);
    let error = encoder.finish().expect_err("a non-zero exit must fail");

    assert!(matches!(error, VideoError::Ffmpeg { .. }), "{error}");
    let message = error.to_string();
    assert!(message.contains("boom: encoder exploded"), "{message}");
    assert!(message.contains("status"), "{message}");
}
