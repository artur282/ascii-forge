//! End-to-end tests for the command line interface.

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use predicates::str::contains;
use serde_json::Value;
use tempfile::TempDir;

const SUBCOMMANDS: &[&str] = &[
    "generate",
    "make",
    "render",
    "preview",
    "import",
    "cache",
    "fonts",
    "doctor",
    "mcp",
    "capabilities",
];

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

struct CacheFixture {
    project: PathBuf,
    cache_files: Vec<PathBuf>,
    artifact: PathBuf,
    config: PathBuf,
}

fn cache_fixture(home: &Path) -> CacheFixture {
    let project = home.join("project");
    let ascii_forge = project.join(".ascii-forge");
    let cache_files = vec![
        ascii_forge.join("cache").join("ai").join("first.json"),
        ascii_forge.join("cache").join("vision").join("second.json"),
    ];
    let artifact = ascii_forge.join("artifacts").join("frame.png");
    let config = config_file(home);

    for cache_file in &cache_files {
        fs::create_dir_all(cache_file.parent().expect("cache namespace"))
            .expect("create cache namespace");
        fs::write(cache_file, br#"{"cached":true}"#).expect("write cache file");
    }
    fs::create_dir_all(artifact.parent().expect("artifact directory"))
        .expect("create artifact directory");
    fs::write(&artifact, b"PNG").expect("write artifact");
    write_config(home, "[render]\nfont_size = 18\n");

    CacheFixture {
        project,
        cache_files,
        artifact,
        config,
    }
}

fn assert_cache_fixture_intact(fixture: &CacheFixture) {
    for cache_file in &fixture.cache_files {
        assert!(
            !cache_file.exists(),
            "cache file must be removed: {cache_file:?}"
        );
    }
    assert!(
        fixture.artifact.is_file(),
        "artifact must survive cache clear"
    );
    assert_eq!(fs::read(&fixture.artifact).expect("read artifact"), b"PNG");
    assert!(fixture.config.is_file(), "config must survive cache clear");
}

#[test]
fn help_lists_every_command() {
    let (_home, mut command) = isolated_command();
    let assert = command.arg("--help").assert().success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).expect("utf-8 help");

    for subcommand in SUBCOMMANDS {
        if *subcommand == "mcp" && !cfg!(feature = "mcp") {
            continue;
        }
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
fn cache_clear_removes_cache_files_and_preserves_artifacts_and_config() {
    let (home, mut command) = isolated_command();
    let fixture = cache_fixture(home.path());

    command
        .current_dir(&fixture.project)
        .args(["cache", "clear"])
        .assert()
        .success()
        .stdout(contains("removed 2 cached entries"));

    assert_cache_fixture_intact(&fixture);
}

#[test]
fn cache_clear_json_reports_the_cleared_path_and_count() {
    let (home, mut command) = isolated_command();
    let fixture = cache_fixture(home.path());

    let assert = command
        .current_dir(&fixture.project)
        .args(["cache", "clear", "--json"])
        .assert()
        .success();
    let report = stdout_json(&assert);

    assert_eq!(report["removed"], 2);
    let path = report["path"].as_str().expect("cleared path is a string");
    assert!(
        path.ends_with(".ascii-forge/cache"),
        "unexpected cleared path: {path}"
    );
    assert_cache_fixture_intact(&fixture);
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
fn import_image_writes_a_valid_scene() {
    let (home, mut command) = isolated_command();
    let input = home.path().join("gradient.png");
    let image = image::RgbaImage::from_fn(64, 32, |x, _| {
        let value = (x * 255 / 64) as u8;
        image::Rgba([value, value, value, 255])
    });
    image.save(&input).expect("save fixture");
    let output = home.path().join("imported.json");
    let assert = command
        .args([
            "import",
            input.to_str().expect("utf-8 path"),
            "--cols",
            "40",
            "--charset",
            "blocks",
            "--color",
            "--json",
            "--out",
            output.to_str().expect("utf-8 path"),
        ])
        .assert()
        .success();
    let report = stdout_json(&assert);
    assert_eq!(report["kind"], "image");
    assert_eq!(report["cols"], 40);
    assert_eq!(report["rows"], 11);
    assert_eq!(report["frames"], 1);
    assert_eq!(report["color"], true);

    let scene: Value =
        serde_json::from_slice(&fs::read(&output).expect("scene file")).expect("scene JSON");
    assert_eq!(scene["canvas"]["cols"], 40);
    assert_eq!(scene["canvas"]["rows"], 11);
    assert_eq!(scene["timeline"]["frames"], 1);
    assert_eq!(scene["output"]["format"], "png");
    assert_eq!(scene["layers"][0]["kind"]["type"], "grid");
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

/// Read the IHDR width and height of a PNG file.
fn png_dimensions(path: &Path) -> (u32, u32) {
    let bytes = fs::read(path).expect("read png");
    assert!(
        bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        "{} is not a PNG",
        path.display()
    );
    let width = u32::from_be_bytes(bytes[16..20].try_into().expect("png width"));
    let height = u32::from_be_bytes(bytes[20..24].try_into().expect("png height"));
    (width, height)
}

#[test]
fn make_writes_a_png_matching_the_reported_dimensions() {
    let (home, mut command) = isolated_command();
    let project = home.path().join("make-png");
    fs::create_dir_all(&project).expect("create project");

    let assert = command
        .current_dir(&project)
        .args([
            "make",
            "NEBULA",
            "--template",
            "logo",
            "--tagline",
            "navigate the noise",
            "--seed",
            "7",
            "--json",
        ])
        .assert()
        .success();
    let report = stdout_json(&assert);

    assert_eq!(report["output"], "nebula.png");
    assert_eq!(report["format"], "png");
    assert_eq!(report["template"], "logo");
    assert_eq!(report["title"], "NEBULA");
    assert_eq!(report["tagline"], "navigate the noise");
    assert_eq!(report["seed"], 7);
    assert_eq!(report["frames"], 60);
    assert_eq!(report["frame"], 59);

    let output = project.join(report["output"].as_str().expect("output path"));
    let (width, height) = png_dimensions(&output);
    assert_eq!(report["width"].as_u64().expect("width"), u64::from(width));
    assert_eq!(
        report["height"].as_u64().expect("height"),
        u64::from(height)
    );
}

#[test]
fn make_defaults_to_the_logo_template_and_a_slug_output() {
    let (home, mut command) = isolated_command();
    let project = home.path().join("make-default");
    fs::create_dir_all(&project).expect("create project");

    let assert = command
        .current_dir(&project)
        .args(["make", "Hello, Forge!", "--json"])
        .assert()
        .success();
    let report = stdout_json(&assert);

    assert_eq!(report["output"], "hello-forge.png");
    assert_eq!(report["template"], "logo");
    assert_eq!(report["tagline"], Value::Null);
    assert!(project.join("hello-forge.png").exists());
}

#[test]
fn make_slug_transliterates_latin_accents() {
    let (home, mut command) = isolated_command();
    let project = home.path().join("make-slug");
    fs::create_dir_all(&project).expect("create project");

    let assert = command
        .current_dir(&project)
        .args(["make", "Canción Ñandú", "--json"])
        .assert()
        .success();
    let report = stdout_json(&assert);

    assert_eq!(report["output"], "cancion-nandu.png");
}

#[test]
fn make_writes_svg_when_requested() {
    let (home, mut command) = isolated_command();
    let project = home.path().join("make-svg");
    fs::create_dir_all(&project).expect("create project");

    let assert = command
        .current_dir(&project)
        .args([
            "make",
            "Orbit",
            "--format",
            "svg",
            "--out",
            "orbit.svg",
            "--json",
        ])
        .assert()
        .success();
    let report = stdout_json(&assert);

    assert_eq!(report["format"], "svg");
    let document = fs::read_to_string(project.join("orbit.svg")).expect("read svg");
    assert!(document.contains("<svg"), "svg output: {document}");
}

#[test]
fn make_creates_parent_directories() {
    let (home, mut command) = isolated_command();
    let project = home.path().join("make-nested");
    fs::create_dir_all(&project).expect("create project");

    command
        .current_dir(&project)
        .args(["make", "Forge", "--out", "nested/deeper/hero.png", "--json"])
        .assert()
        .success();

    assert!(project.join("nested/deeper/hero.png").exists());
}

#[test]
fn make_rejects_non_static_formats() {
    let (_home, mut command) = isolated_command();
    command
        .args(["make", "Forge", "--format", "gif"])
        .assert()
        .failure()
        .stderr(contains("expected png, jpeg or svg"));
}

#[test]
fn make_rejects_a_blank_title() {
    let (_home, mut command) = isolated_command();
    command
        .args(["make", "   "])
        .assert()
        .failure()
        .stderr(contains("title must not be empty"));
}

#[test]
fn capabilities_json_exposes_the_agent_contract() {
    let (_home, mut command) = isolated_command();
    let assert = command.args(["capabilities", "--json"]).assert().success();
    let report = stdout_json(&assert);

    assert_eq!(report["name"], "ascii-forge");
    assert_eq!(report["version"], env!("CARGO_PKG_VERSION"));

    let names: Vec<&str> = report["commands"]
        .as_array()
        .expect("commands")
        .iter()
        .map(|command| command["name"].as_str().expect("command name"))
        .collect();
    for expected in [
        "generate",
        "make",
        "render",
        "preview",
        "import",
        "cache",
        "fonts",
        "doctor",
        "capabilities",
    ] {
        assert!(names.contains(&expected), "missing command {expected}");
    }
    assert_eq!(names.contains(&"mcp"), cfg!(feature = "mcp"));

    let templates: Vec<&str> = report["templates"]
        .as_array()
        .expect("templates")
        .iter()
        .map(|template| template["name"].as_str().expect("template name"))
        .collect();
    assert_eq!(
        templates,
        ["logo", "promo", "glitch", "matrix", "lower-third"]
    );
    assert_eq!(report["effects"].as_array().expect("effects").len(), 8);
    assert_eq!(report["charsets"].as_array().expect("charsets").len(), 3);
    assert_eq!(report["formats"]["static"][0], "png");
    assert!(report["examples"].as_array().expect("examples").len() >= 3);
}

#[test]
fn capabilities_human_output_lists_commands_and_templates() {
    let (_home, mut command) = isolated_command();
    let assert = command.arg("capabilities").assert().success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).expect("utf-8 output");

    assert!(stdout.contains("make"), "output: {stdout}");
    assert!(stdout.contains("lower-third"), "output: {stdout}");
}

#[test]
fn make_accepts_the_scene_template_spelling() {
    let (home, mut command) = isolated_command();
    let project = home.path().join("make-scene-name");
    fs::create_dir_all(&project).expect("create project");

    let assert = command
        .current_dir(&project)
        .args(["make", "Live", "--template", "lower_third", "--json"])
        .assert()
        .success();

    assert_eq!(stdout_json(&assert)["template"], "lower_third");
}

#[test]
fn make_reports_the_settled_frame_for_every_template() {
    let (home, _command) = isolated_command();
    let project = home.path().join("make-templates");
    fs::create_dir_all(&project).expect("create project");

    for template in ["logo", "promo", "glitch", "matrix", "lower-third"] {
        let output = format!("{template}.png");
        let (_run_home, mut run) = isolated_command();
        let assert = run
            .current_dir(&project)
            .args([
                "make",
                "Probe",
                "--template",
                template,
                "--tagline",
                "tag",
                "--out",
                output.as_str(),
                "--json",
            ])
            .assert()
            .success();
        let report = stdout_json(&assert);
        assert!(
            report["template"]
                .as_str()
                .is_some_and(|name| !name.is_empty())
        );
        assert_eq!(report["tagline"], "tag");
        assert_eq!(
            report["frame"].as_u64().expect("frame") + 1,
            report["frames"].as_u64().expect("frames")
        );
        let (width, height) = png_dimensions(&project.join(&output));
        assert_eq!(report["width"].as_u64().expect("width"), u64::from(width));
        assert_eq!(
            report["height"].as_u64().expect("height"),
            u64::from(height)
        );
    }
}

#[test]
fn make_slug_caps_at_48_chars_and_falls_back_to_image() {
    let (home, mut command) = isolated_command();
    let project = home.path().join("make-slug-limits");
    fs::create_dir_all(&project).expect("create project");

    let long = "a".repeat(60);
    let assert = command
        .current_dir(&project)
        .args(["make", long.as_str(), "--json"])
        .assert()
        .success();
    let expected = format!("{}.png", "a".repeat(48));
    assert_eq!(stdout_json(&assert)["output"], expected.as_str());

    let (_run_home, mut run) = isolated_command();
    let assert = run
        .current_dir(&project)
        .args(["make", "!!!", "--json"])
        .assert()
        .success();
    assert_eq!(stdout_json(&assert)["output"], "image.png");
}

#[test]
fn make_jpeg_uses_jpg_and_reports_real_dimensions() {
    let (home, mut command) = isolated_command();
    let project = home.path().join("make-jpeg");
    fs::create_dir_all(&project).expect("create project");

    let assert = command
        .current_dir(&project)
        .args(["make", "Jpeg", "--format", "jpeg", "--json"])
        .assert()
        .success();
    let report = stdout_json(&assert);

    assert_eq!(report["format"], "jpeg");
    assert_eq!(report["output"], "jpeg.jpg");
    let (width, height) =
        image::image_dimensions(project.join("jpeg.jpg")).expect("jpeg dimensions");
    assert_eq!(report["width"].as_u64().expect("width"), u64::from(width));
    assert_eq!(
        report["height"].as_u64().expect("height"),
        u64::from(height)
    );
}

#[test]
fn make_default_frame_is_the_rendered_last_frame() {
    let (home, mut command) = isolated_command();
    let project = home.path().join("make-settled");
    fs::create_dir_all(&project).expect("create project");

    command
        .current_dir(&project)
        .args(["make", "Nebula", "--out", "made.png"])
        .assert()
        .success();

    let (_generate_home, mut generate) = isolated_command();
    generate
        .current_dir(&project)
        .args([
            "generate",
            "--template",
            "logo",
            "--title",
            "Nebula",
            "--out",
            "scene.json",
        ])
        .assert()
        .success();

    for (frame, output) in [("59", "last.png"), ("0", "first.png")] {
        let (_render_home, mut render) = isolated_command();
        render
            .current_dir(&project)
            .args([
                "render",
                "scene.json",
                "--format",
                "png",
                "--frame",
                frame,
                "--out",
                output,
            ])
            .assert()
            .success();
    }

    let made = fs::read(project.join("made.png")).expect("made png");
    assert_eq!(made, fs::read(project.join("last.png")).expect("last png"));
    assert_ne!(
        made,
        fs::read(project.join("first.png")).expect("first png")
    );
}

#[test]
fn rejected_make_commands_write_nothing() {
    let (home, mut command) = isolated_command();
    let project = home.path().join("make-rejected");
    fs::create_dir_all(&project).expect("create project");

    command
        .current_dir(&project)
        .args(["make", "Forge", "--format", "gif"])
        .assert()
        .failure();

    let (_run_home, mut run) = isolated_command();
    run.current_dir(&project)
        .args(["make", "   "])
        .assert()
        .failure();

    assert_eq!(fs::read_dir(&project).expect("list").count(), 0);
}

#[cfg(not(feature = "mcp"))]
#[test]
fn capabilities_omit_mcp_without_the_feature() {
    let (_home, mut command) = isolated_command();
    let assert = command.args(["capabilities", "--json"]).assert().success();
    let report = stdout_json(&assert);
    let names: Vec<&str> = report["commands"]
        .as_array()
        .expect("commands")
        .iter()
        .map(|command| command["name"].as_str().expect("command name"))
        .collect();

    assert!(!names.contains(&"mcp"));
    assert_eq!(names.len(), 9);
}
