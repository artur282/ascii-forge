//! Video/GIF import tests; skipped when FFmpeg is unavailable.

use std::path::Path;
use std::process::Command;

use ascii_forge::import::video::{default_ffprobe, import_video};
use ascii_forge::import::{ImportOptions, build_video_scene};
use ascii_forge::scene::OutputFormat;

fn ffmpeg_available() -> bool {
    Command::new("ffmpeg")
        .arg("-version")
        .output()
        .is_ok_and(|output| output.status.success())
        && Command::new("ffprobe")
            .arg("-version")
            .output()
            .is_ok_and(|output| output.status.success())
}

fn make_video(path: &Path) {
    let status = Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc=duration=0.4:size=32x24:rate=10",
            "-pix_fmt",
            "yuv420p",
            "-c:v",
            "mpeg4",
            "-y",
        ])
        .arg(path)
        .status()
        .expect("run ffmpeg");
    assert!(status.success(), "ffmpeg fixture generation failed");
}

fn make_gif(path: &Path) {
    let status = Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc=duration=0.3:size=32x24:rate=10",
            "-vf",
            "fps=5",
            "-y",
        ])
        .arg(path)
        .status()
        .expect("run ffmpeg");
    assert!(status.success(), "ffmpeg GIF fixture generation failed");
}

#[test]
fn imports_short_video_frames() {
    if !ffmpeg_available() {
        eprintln!("skipping video import test: ffmpeg/ffprobe unavailable");
        return;
    }
    let dir = tempfile::tempdir().expect("temp dir");
    let input = dir.path().join("fixture.mp4");
    make_video(&input);
    let options = ImportOptions {
        cols: 20,
        max_frames: 4,
        fps: 5,
        ..ImportOptions::default()
    };
    let imported = import_video(
        &input,
        &options,
        Path::new("ffmpeg"),
        &default_ffprobe(Path::new("ffmpeg")),
    )
    .expect("video import");

    assert!(!imported.frames.is_empty());
    assert!(
        imported.frames.len() <= 4,
        "got {} frames",
        imported.frames.len()
    );
    assert_eq!(imported.frames[0].cols, 20);
    assert_eq!(imported.frames[0].rows, 8);
    assert_eq!(imported.width, 32);
    assert_eq!(imported.height, 24);
}

#[test]
fn builds_a_valid_gif_scene_from_video_frames() {
    if !ffmpeg_available() {
        eprintln!("skipping video scene test: ffmpeg/ffprobe unavailable");
        return;
    }
    let dir = tempfile::tempdir().expect("temp dir");
    let input = dir.path().join("fixture.mp4");
    make_video(&input);
    let options = ImportOptions {
        cols: 20,
        max_frames: 3,
        fps: 5,
        color: true,
        ..ImportOptions::default()
    };
    let imported = import_video(
        &input,
        &options,
        Path::new("ffmpeg"),
        &default_ffprobe(Path::new("ffmpeg")),
    )
    .expect("video import");
    let frame_count = imported.frames.len();
    let scene = build_video_scene(imported.frames, imported.fps, &options).expect("scene");

    assert_eq!(scene.output.format, OutputFormat::Gif);
    assert_eq!(scene.timeline.frames as usize, frame_count);
    assert_eq!(scene.timeline.fps, 5);
    assert!(scene.validate().is_ok());
}

#[test]
fn imports_a_gif() {
    if !ffmpeg_available() {
        eprintln!("skipping GIF import test: ffmpeg/ffprobe unavailable");
        return;
    }
    let dir = tempfile::tempdir().expect("temp dir");
    let input = dir.path().join("fixture.gif");
    make_gif(&input);
    let options = ImportOptions {
        cols: 16,
        max_frames: 3,
        fps: 5,
        braille: true,
        ..ImportOptions::default()
    };
    let imported = import_video(
        &input,
        &options,
        Path::new("ffmpeg"),
        &default_ffprobe(Path::new("ffmpeg")),
    )
    .expect("GIF import");

    assert!(!imported.frames.is_empty());
    assert_eq!(imported.frames[0].cols, 16);
    assert!(imported.frames.iter().all(|frame| {
        frame
            .cells
            .iter()
            .all(|cell| (0x2800..=0x28ff).contains(&(cell.ch as u32)))
    }));
}
