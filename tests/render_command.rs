use std::fs;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;

use assert_cmd::Command;
use serde_json::json;
use tempfile::TempDir;

fn scene() -> serde_json::Value {
    json!({
        "version": 1,
        "canvas": {
            "cols": 80,
            "rows": 24,
            "cell_aspect": 0.542,
            "background": { "r": 10, "g": 12, "b": 16, "a": 255 },
            "foreground": { "r": 0, "g": 255, "b": 136, "a": 255 }
        },
        "layers": [
            {
                "id": "logo",
                "kind": { "type": "figlet", "text": "AF", "font": "standard" },
                "transform": { "x": 0, "y": 0 }
            },
            {
                "id": "tag",
                "kind": { "type": "text", "text": "HI", "align": "center" },
                "transform": { "x": 0, "y": 8 }
            }
        ],
        "timeline": { "fps": 30, "frames": 1, "loop": false },
        "effects": [],
        "output": { "width": 640, "height": 480, "format": "png" }
    })
}

fn write_scene(dir: &Path) -> PathBuf {
    let path = dir.join("scene.json");
    fs::write(&path, serde_json::to_vec_pretty(&scene()).unwrap()).unwrap();
    path
}

fn command(dir: &Path) -> Command {
    let mut command = Command::cargo_bin("ascii-forge").unwrap();
    command
        .env("HOME", dir)
        .env("XDG_CONFIG_HOME", dir.join("config"))
        .env_remove("ASCII_FORGE_FONT")
        .env_remove("ASCII_FORGE_FONT_SIZE")
        .env_remove("ASCII_FORGE_FFMPEG")
        .env_remove("NO_COLOR");
    command
}

fn tool_available(tool: &str) -> bool {
    StdCommand::new(tool)
        .arg("-version")
        .output()
        .is_ok_and(|output| output.status.success())
}

fn ffmpeg_ready() -> bool {
    tool_available("ffmpeg") && tool_available("ffprobe")
}

fn has_encoder(name: &str) -> bool {
    let Ok(output) = StdCommand::new("ffmpeg")
        .args(["-hide_banner", "-encoders"])
        .output()
    else {
        return false;
    };
    output.status.success()
        && String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter_map(|line| line.split_whitespace().nth(1))
            .any(|encoder| encoder == name)
}

fn probe_json(path: &Path) -> serde_json::Value {
    let output = StdCommand::new("ffprobe")
        .args([
            "-v",
            "error",
            "-print_format",
            "json",
            "-show_streams",
            "-show_format",
            "-count_frames",
        ])
        .arg(path)
        .output()
        .expect("run ffprobe");
    assert!(
        output.status.success(),
        "ffprobe failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("ffprobe JSON")
}

fn write_video_scene(dir: &Path, frames: u32, width: u32, height: u32) -> PathBuf {
    let mut value = scene();
    value["timeline"]["frames"] = json!(frames);
    value["output"]["width"] = json!(width);
    value["output"]["height"] = json!(height);
    let path = dir.join("video.json");
    fs::write(&path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
    path
}

fn stream<'a>(probe: &'a serde_json::Value, kind: &str) -> &'a serde_json::Value {
    probe["streams"]
        .as_array()
        .expect("streams array")
        .iter()
        .find(|stream| stream["codec_type"] == kind)
        .unwrap_or_else(|| panic!("missing {kind} stream"))
}

#[test]
fn render_png_writes_an_image() {
    let dir = TempDir::new().unwrap();
    let scene_path = write_scene(dir.path());
    let output = dir.path().join("logo.png");

    command(dir.path())
        .args([
            "render",
            scene_path.to_str().unwrap(),
            "--format",
            "png",
            "--out",
            output.to_str().unwrap(),
        ])
        .assert()
        .success();

    let image = image::open(&output).expect("render must produce a valid PNG");
    assert_eq!(image.width(), 640);
    assert_eq!(image.height(), 480);
}

#[test]
fn render_derives_the_output_path_from_the_scene() {
    let dir = TempDir::new().unwrap();
    let scene_path = write_scene(dir.path());

    command(dir.path())
        .args(["render", scene_path.to_str().unwrap(), "--format", "png"])
        .assert()
        .success();

    assert!(scene_path.with_extension("png").is_file());
}

#[test]
fn render_svg_prints_a_document() {
    let dir = TempDir::new().unwrap();
    let scene_path = write_scene(dir.path());

    let assertion = command(dir.path())
        .args(["render", scene_path.to_str().unwrap(), "--format", "svg"])
        .assert()
        .success();

    let stdout = String::from_utf8_lossy(&assertion.get_output().stdout);
    assert!(stdout.contains("<svg"));
    assert!(stdout.contains("<text"));
}

#[test]
fn render_text_prints_the_layer_content() {
    let dir = TempDir::new().unwrap();
    let scene_path = write_scene(dir.path());

    let assertion = command(dir.path())
        .args(["render", scene_path.to_str().unwrap(), "--format", "text"])
        .assert()
        .success();

    let stdout = String::from_utf8_lossy(&assertion.get_output().stdout);
    assert!(stdout.contains("HI"), "unexpected output: {stdout}");
}

#[test]
fn render_json_reports_the_artifact() {
    let dir = TempDir::new().unwrap();
    let scene_path = write_scene(dir.path());
    let output = dir.path().join("logo.png");

    let assertion = command(dir.path())
        .args([
            "render",
            scene_path.to_str().unwrap(),
            "--format",
            "png",
            "--out",
            output.to_str().unwrap(),
            "--json",
        ])
        .assert()
        .success();

    let report: serde_json::Value =
        serde_json::from_slice(&assertion.get_output().stdout).expect("valid JSON report");
    assert_eq!(report["format"], "png");
    assert_eq!(report["output"], output.display().to_string());
}

#[test]
fn render_rejects_an_invalid_scene() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("bad.json");
    let mut bad = scene();
    bad["version"] = json!(999);
    fs::write(&path, serde_json::to_vec_pretty(&bad).unwrap()).unwrap();

    let assertion = command(dir.path())
        .args(["render", path.to_str().unwrap(), "--format", "png"])
        .assert()
        .failure();

    let stderr = String::from_utf8_lossy(&assertion.get_output().stderr);
    assert!(
        stderr.contains("unsupported scene version"),
        "unexpected stderr: {stderr}"
    );
}

#[test]
fn render_gif_streams_every_timeline_frame() {
    let dir = TempDir::new().unwrap();
    let mut value = scene();
    value["timeline"]["frames"] = json!(3);
    value["output"]["width"] = json!(64);
    value["output"]["height"] = json!(64);
    value["output"]["format"] = json!("gif");
    let scene_path = dir.path().join("animated.json");
    fs::write(&scene_path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
    let output = dir.path().join("animated.gif");

    command(dir.path())
        .args([
            "render",
            scene_path.to_str().unwrap(),
            "--format",
            "gif",
            "--gif-colors",
            "8",
            "--gif-transparent",
            "--out",
            output.to_str().unwrap(),
        ])
        .assert()
        .success();

    let file = File::open(&output).expect("open gif");
    let mut decoder = gif::Decoder::new(BufReader::new(file)).expect("decode gif header");
    assert_eq!((decoder.width(), decoder.height()), (64, 64));
    let mut frames = 0;
    while let Some(frame) = decoder.read_next_frame().expect("decode frame") {
        frames += 1;
        assert_eq!((frame.width, frame.height), (64, 64));
        if let Some(palette) = &frame.palette {
            assert!(palette.len() / 3 <= 8, "palette exceeds --gif-colors");
        }
        assert!(
            frame.transparent.is_some(),
            "--gif-transparent must keep transparent pixels"
        );
    }
    assert_eq!(frames, 3);
}

#[test]
fn render_rejects_an_out_of_range_frame() {
    let dir = TempDir::new().unwrap();
    let scene_path = write_scene(dir.path());

    let assertion = command(dir.path())
        .args([
            "render",
            scene_path.to_str().unwrap(),
            "--format",
            "png",
            "--frame",
            "99",
        ])
        .assert()
        .failure();

    let stderr = String::from_utf8_lossy(&assertion.get_output().stderr);
    assert!(
        stderr.contains("outside the timeline"),
        "unexpected stderr: {stderr}"
    );
}

#[test]
fn render_rejects_an_invalid_gif_palette() {
    let dir = TempDir::new().unwrap();
    let scene_path = write_scene(dir.path());

    let assertion = command(dir.path())
        .args([
            "render",
            scene_path.to_str().unwrap(),
            "--format",
            "gif",
            "--gif-colors",
            "1",
        ])
        .assert()
        .failure();

    let stderr = String::from_utf8_lossy(&assertion.get_output().stderr);
    assert!(
        stderr.contains("GIF palette size"),
        "unexpected stderr: {stderr}"
    );
}

#[test]
fn preview_prints_one_frame_without_a_terminal() {
    let dir = TempDir::new().unwrap();
    let scene_path = write_scene(dir.path());

    let assertion = command(dir.path())
        .args([
            "preview",
            scene_path.to_str().unwrap(),
            "--frame",
            "0",
            "--no-color",
        ])
        .assert()
        .success();

    let stdout = String::from_utf8_lossy(&assertion.get_output().stdout);
    assert!(stdout.contains("HI"), "unexpected preview: {stdout}");
}

#[test]
fn preview_rejects_json_output() {
    let dir = TempDir::new().unwrap();
    let scene_path = write_scene(dir.path());

    let assertion = command(dir.path())
        .args(["preview", scene_path.to_str().unwrap(), "--json"])
        .assert()
        .failure();

    let stderr = String::from_utf8_lossy(&assertion.get_output().stderr);
    assert!(
        stderr.contains("does not support --json"),
        "unexpected stderr: {stderr}"
    );
}

#[test]
fn render_mp4_streams_every_timeline_frame() {
    if !ffmpeg_ready() {
        eprintln!("skipping MP4 integration test: ffmpeg/ffprobe unavailable");
        return;
    }
    let dir = TempDir::new().unwrap();
    let scene_path = write_video_scene(dir.path(), 6, 64, 64);
    let output = dir.path().join("clip.mp4");

    command(dir.path())
        .args([
            "render",
            scene_path.to_str().unwrap(),
            "--format",
            "mp4",
            "--crf",
            "28",
            "--preset",
            "ultrafast",
            "--out",
            output.to_str().unwrap(),
        ])
        .assert()
        .success();

    let probe = probe_json(&output);
    let video = stream(&probe, "video");
    assert_eq!(video["codec_name"], "h264");
    assert_eq!(video["width"], 64);
    assert_eq!(video["height"], 64);
    assert_eq!(video["nb_read_frames"], "6");
    assert_eq!(video["avg_frame_rate"], "30/1");
}

#[test]
fn render_webm_uses_vp9_when_available() {
    if !ffmpeg_ready() || !has_encoder("libvpx-vp9") {
        eprintln!("skipping WebM integration test: libvpx-vp9 unavailable");
        return;
    }
    let dir = TempDir::new().unwrap();
    let scene_path = write_video_scene(dir.path(), 6, 64, 64);
    let output = dir.path().join("clip.webm");

    command(dir.path())
        .args([
            "render",
            scene_path.to_str().unwrap(),
            "--format",
            "webm",
            "--preset",
            "ultrafast",
            "--out",
            output.to_str().unwrap(),
        ])
        .assert()
        .success();

    let probe = probe_json(&output);
    let video = stream(&probe, "video");
    assert_eq!(video["codec_name"], "vp9");
    assert_eq!(video["width"], 64);
    assert_eq!(video["height"], 64);
    assert_eq!(video["nb_read_frames"], "6");
}

#[test]
fn render_pads_odd_video_dimensions() {
    if !ffmpeg_ready() {
        eprintln!("skipping odd-dimension integration test: ffmpeg/ffprobe unavailable");
        return;
    }
    let dir = TempDir::new().unwrap();
    let scene_path = write_video_scene(dir.path(), 3, 63, 63);
    let output = dir.path().join("odd.mp4");

    command(dir.path())
        .args([
            "render",
            scene_path.to_str().unwrap(),
            "--format",
            "mp4",
            "--preset",
            "ultrafast",
            "--width",
            "63",
            "--height",
            "63",
            "--out",
            output.to_str().unwrap(),
        ])
        .assert()
        .success();

    let probe = probe_json(&output);
    let video = stream(&probe, "video");
    assert_eq!(video["width"], 64);
    assert_eq!(video["height"], 64);
    assert_eq!(video["nb_read_frames"], "3");
}

#[test]
fn render_muxes_an_audio_track_into_mp4() {
    if !ffmpeg_ready() {
        eprintln!("skipping audio integration test: ffmpeg/ffprobe unavailable");
        return;
    }
    let dir = TempDir::new().unwrap();
    let scene_path = write_video_scene(dir.path(), 30, 64, 64);
    let tone = dir.path().join("tone.wav");
    let status = StdCommand::new("ffmpeg")
        .args([
            "-y",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=1",
            "-ar",
            "44100",
            "-ac",
            "1",
        ])
        .arg(&tone)
        .status()
        .expect("generate tone");
    assert!(status.success(), "ffmpeg could not generate the test tone");
    let output = dir.path().join("with-audio.mp4");

    command(dir.path())
        .args([
            "render",
            scene_path.to_str().unwrap(),
            "--format",
            "mp4",
            "--preset",
            "ultrafast",
            "--audio",
            tone.to_str().unwrap(),
            "--out",
            output.to_str().unwrap(),
        ])
        .assert()
        .success();

    let probe = probe_json(&output);
    assert_eq!(stream(&probe, "video")["codec_name"], "h264");
    assert_eq!(stream(&probe, "audio")["codec_name"], "aac");
}

#[test]
fn render_synthesizes_a_voice_over_into_mp4() {
    if !ffmpeg_ready() || !tool_available("espeak-ng") {
        eprintln!("skipping voice integration test: ffmpeg/ffprobe/espeak-ng unavailable");
        return;
    }
    let dir = TempDir::new().unwrap();
    let scene_path = write_video_scene(dir.path(), 30, 64, 64);
    let output = dir.path().join("voice.mp4");

    command(dir.path())
        .args([
            "render",
            scene_path.to_str().unwrap(),
            "--format",
            "mp4",
            "--preset",
            "ultrafast",
            "--voice",
            "Welcome to ASCII Forge",
            "--out",
            output.to_str().unwrap(),
        ])
        .assert()
        .success();

    let probe = probe_json(&output);
    assert_eq!(stream(&probe, "video")["codec_name"], "h264");
    assert_eq!(stream(&probe, "audio")["codec_name"], "aac");
}

#[test]
fn render_rejects_audio_for_webm() {
    let dir = TempDir::new().unwrap();
    let scene_path = write_scene(dir.path());

    let assertion = command(dir.path())
        .args([
            "render",
            scene_path.to_str().unwrap(),
            "--format",
            "webm",
            "--audio",
            "track.wav",
        ])
        .assert()
        .failure();

    let stderr = String::from_utf8_lossy(&assertion.get_output().stderr);
    assert!(
        stderr.contains("only supported for MP4"),
        "unexpected stderr: {stderr}"
    );
}

#[test]
fn render_reports_a_missing_ffmpeg() {
    let dir = TempDir::new().unwrap();
    let scene_path = write_video_scene(dir.path(), 1, 64, 64);
    let output = dir.path().join("missing.mp4");

    let assertion = command(dir.path())
        .args([
            "render",
            scene_path.to_str().unwrap(),
            "--format",
            "mp4",
            "--ffmpeg-path",
            "/definitely/not/ffmpeg",
            "--out",
            output.to_str().unwrap(),
        ])
        .assert()
        .failure();

    let stderr = String::from_utf8_lossy(&assertion.get_output().stderr);
    assert!(
        stderr.contains("ffmpeg") && stderr.contains("/definitely/not/ffmpeg"),
        "unexpected stderr: {stderr}"
    );
}

#[test]
fn render_video_json_reports_timeline_metadata() {
    if !ffmpeg_ready() {
        eprintln!("skipping video JSON integration test: ffmpeg/ffprobe unavailable");
        return;
    }
    let dir = TempDir::new().unwrap();
    let scene_path = write_video_scene(dir.path(), 6, 64, 64);
    let output = dir.path().join("report.mp4");

    let assertion = command(dir.path())
        .args([
            "render",
            scene_path.to_str().unwrap(),
            "--format",
            "mp4",
            "--preset",
            "ultrafast",
            "--out",
            output.to_str().unwrap(),
            "--json",
        ])
        .assert()
        .success();

    let report: serde_json::Value =
        serde_json::from_slice(&assertion.get_output().stdout).expect("valid JSON report");
    assert_eq!(report["format"], "mp4");
    assert_eq!(report["frames"], 6);
    assert_eq!(report["fps"], 30);
    assert_eq!(report["width"], 64);
    assert_eq!(report["height"], 64);
    assert_eq!(report["audio"], false);
}
