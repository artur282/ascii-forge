//! End-to-end tests for the command line interface.

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use predicates::prelude::*;
use predicates::str::contains;
use serde_json::Value;
use tempfile::TempDir;

const SUBCOMMANDS: &[&str] = &["generate", "render", "preview", "import", "fonts", "doctor"];

const DOCTOR_KEYS: &[&str] = &[
    "ffmpeg",
    "ffprobe",
    "libx264",
    "libvpx_vp9",
    "font",
    "config_dir",
];

/// Build the binary with an isolated home so the developer's config never leaks in.
fn isolated_command() -> (TempDir, Command) {
    let home = tempfile::tempdir().expect("create temporary home");
    let mut command = Command::cargo_bin("ascii-forge").expect("ascii-forge binary");
    command
        .env("XDG_CONFIG_HOME", home.path())
        .env("HOME", home.path())
        .env_remove("ASCII_FORGE_FFMPEG")
        .env_remove("ASCII_FORGE_FONT")
        .env_remove("ASCII_FORGE_FONT_SIZE")
        .env_remove("ASCII_FORGE_API_KEY")
        .env_remove("OPENAI_API_KEY")
        .env_remove("NO_COLOR");
    (home, command)
}

fn config_file(home: &Path) -> PathBuf {
    home.join("ascii-forge").join("config.toml")
}

fn write_config(home: &Path, contents: &str) {
    let path = config_file(home);
    fs::create_dir_all(path.parent().expect("config directory")).expect("create config directory");
    fs::write(path, contents).expect("write config file");
}

fn stdout_json(assert: &assert_cmd::assert::Assert) -> Value {
    serde_json::from_slice(&assert.get_output().stdout).expect("stdout is valid JSON")
}

#[test]
fn help_lists_every_command() {
    let (_home, mut command) = isolated_command();
    let assert = command.arg("--help").assert().success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).expect("utf-8 help");

    for subcommand in SUBCOMMANDS {
        assert!(
            stdout.contains(subcommand),
            "help output must list `{subcommand}`:\n{stdout}"
        );
    }
}

#[test]
fn version_is_available() {
    let (_home, mut command) = isolated_command();
    command
        .arg("--version")
        .assert()
        .success()
        .stdout(contains("ascii-forge"));
}

#[test]
fn fonts_lists_the_figlet_catalog() {
    let (_home, mut command) = isolated_command();
    command
        .arg("fonts")
        .assert()
        .success()
        .stdout(contains("standard"));
}

#[test]
fn pending_commands_report_not_implemented() {
    let (_home, mut command) = isolated_command();
    command
        .args([
            "import",
            "photo.png",
            "--output",
            "scene.json",
            "--charset",
            "@#* ",
        ])
        .assert()
        .code(1)
        .stdout(predicate::str::is_empty())
        .stderr(contains("not implemented yet"));
}

#[test]
fn generate_template_writes_a_scene_without_network() {
    let (home, mut command) = isolated_command();
    let output = home.path().join("logo.json");
    let assert = command
        .args([
            "generate",
            "--template",
            "logo",
            "--title",
            "NEBULA",
            "--tagline",
            "navigate the noise",
            "--seed",
            "7",
            "--json",
            "--out",
            output.to_str().expect("utf-8 path"),
        ])
        .assert()
        .success();
    let report = stdout_json(&assert);
    assert_eq!(report["source"], "template");
    assert_eq!(report["template"], "logo");
    assert_eq!(report["seed"], 7);
    assert_eq!(report["attempts"], 0);
    assert_eq!(report["cached"], false);

    let scene: Value =
        serde_json::from_slice(&fs::read(&output).expect("scene file")).expect("scene JSON");
    assert_eq!(scene["version"], 1);
    assert_eq!(scene["seed"], 7);
    assert_eq!(scene["timeline"]["frames"], 60);
    assert_eq!(scene["layers"][0]["kind"]["type"], "figlet");
    assert_eq!(scene["layers"][0]["kind"]["text"], "NEBULA");
}

#[test]
fn generate_without_a_key_or_template_fails_clearly() {
    let (home, mut command) = isolated_command();
    let output = home.path().join("scene.json");
    command
        .args([
            "generate",
            "a neon city",
            "--out",
            output.to_str().expect("utf-8 path"),
        ])
        .assert()
        .code(1)
        .stderr(contains("missing API key"));
}

#[test]
fn generate_template_requires_a_title() {
    let (home, mut command) = isolated_command();
    let output = home.path().join("scene.json");
    command
        .args([
            "generate",
            "--template",
            "promo",
            "--out",
            output.to_str().expect("utf-8 path"),
        ])
        .assert()
        .code(1)
        .stderr(contains("requires --title"));
}

#[test]
fn generate_vision_requires_a_configured_model() {
    let (home, mut command) = isolated_command();
    let output = home.path().join("scene.json");
    command
        .args([
            "generate",
            "--template",
            "logo",
            "--title",
            "NEBULA",
            "--vision",
            "--out",
            output.to_str().expect("utf-8 path"),
        ])
        .assert()
        .code(1)
        .stderr(contains("--vision requires"));
}

#[test]
fn doctor_json_reports_every_check() {
    let (_home, mut command) = isolated_command();
    let assert = command.args(["doctor", "--json"]).assert().success();
    let report = stdout_json(&assert);
    let object = report.as_object().expect("doctor report is a JSON object");

    for key in DOCTOR_KEYS {
        let check = object
            .get(*key)
            .unwrap_or_else(|| panic!("doctor report is missing `{key}`: {report}"));
        let status = check
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or_else(|| panic!("`{key}` must report a status: {report}"));
        assert!(
            matches!(status, "ok" | "missing"),
            "unexpected status `{status}` for `{key}`"
        );
        assert!(
            check.get("detail").is_some(),
            "`{key}` must carry a detail field: {report}"
        );
    }

    assert_eq!(
        object["config_dir"]["status"], "ok",
        "temporary config directory must be writable: {report}"
    );
}

#[test]
fn doctor_still_succeeds_when_required_tools_are_missing() {
    let (_home, mut command) = isolated_command();
    let assert = command
        .env("PATH", "")
        .args(["doctor", "--json"])
        .assert()
        .success();
    let report = stdout_json(&assert);

    assert_eq!(report["ffmpeg"]["status"], "missing");
    assert_eq!(report["ffprobe"]["status"], "missing");
    assert_eq!(report["libx264"]["status"], "missing");
    assert_eq!(report["libvpx_vp9"]["status"], "missing");
}

#[test]
fn config_font_path_is_reported_by_fonts() {
    let (home, mut command) = isolated_command();
    let font = home.path().join("configured-mono.ttf");
    fs::write(&font, b"placeholder").expect("write font file");
    write_config(
        home.path(),
        &format!("[render]\nfont_path = \"{}\"\n", font.display()),
    );

    let assert = command.args(["fonts", "--json"]).assert().success();
    let report = stdout_json(&assert);

    assert_eq!(report["font_path"], font.to_string_lossy().as_ref());
}

#[test]
fn environment_overrides_config_font_path() {
    let (home, mut command) = isolated_command();
    let configured = home.path().join("configured-mono.ttf");
    let from_env = home.path().join("env-mono.ttf");
    fs::write(&configured, b"placeholder").expect("write configured font");
    fs::write(&from_env, b"placeholder").expect("write env font");
    write_config(
        home.path(),
        &format!("[render]\nfont_path = \"{}\"\n", configured.display()),
    );

    let assert = command
        .env("ASCII_FORGE_FONT", &from_env)
        .args(["fonts", "--json"])
        .assert()
        .success();
    let report = stdout_json(&assert);

    assert_eq!(report["font_path"], from_env.to_string_lossy().as_ref());
}

#[test]
fn config_rejects_api_keys() {
    let (home, mut command) = isolated_command();
    write_config(
        home.path(),
        "[ai]\nprovider = \"openai\"\napi_key = \"secret\"\n",
    );

    command
        .arg("fonts")
        .assert()
        .failure()
        .stderr(contains("api_key"));
}
