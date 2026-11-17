#![cfg(feature = "mcp")]

//! End-to-end tests for the MCP stdio server (Phase 6, spec S1).
//!
//! Every test drives a real `ascii-forge mcp` child over newline-delimited
//! JSON-RPC on stdin/stdout. No network or API key is used: scenes come from
//! offline templates.

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tempfile::TempDir;

/// Upper bound for one request/response round trip.
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(30);

/// Upper bound for the child to exit after stdin closes.
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(10);

/// The tools the server must always advertise.
const TOOL_NAMES: [&str; 5] = [
    "create_scene",
    "render_preview",
    "export",
    "list_fonts",
    "doctor",
];

/// Isolated project and home directories for one test.
struct TestProject {
    project: TempDir,
    home: TempDir,
}

impl TestProject {
    fn new() -> Self {
        Self {
            project: tempfile::tempdir().expect("create temporary project"),
            home: tempfile::tempdir().expect("create temporary home"),
        }
    }

    fn root(&self) -> &Path {
        self.project.path()
    }

    fn client(&self) -> McpClient {
        McpClient::spawn(self.root(), self.home.path())
    }
}

/// Minimal blocking MCP client for newline-delimited JSON-RPC over stdio.
struct McpClient {
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    lines: Receiver<String>,
    next_id: u64,
}

impl McpClient {
    fn spawn(project: &Path, home: &Path) -> Self {
        let mut child = Command::new(assert_cmd::cargo::cargo_bin("ascii-forge"))
            .arg("mcp")
            .current_dir(project)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // Diagnostics go to the test's stderr; server stdout stays protocol-only.
            .stderr(Stdio::inherit())
            .env("HOME", home)
            .env("XDG_CONFIG_HOME", home)
            .env_remove("ASCII_FORGE_API_KEY")
            .env_remove("OPENAI_API_KEY")
            .env_remove("ASCII_FORGE_ALLOW_INSECURE_HTTP")
            .env_remove("ASCII_FORGE_FFMPEG")
            .env_remove("ASCII_FORGE_FONT")
            .env_remove("ASCII_FORGE_FONT_SIZE")
            .env_remove("NO_COLOR")
            .spawn()
            .expect("spawn `ascii-forge mcp`");

        let stdin = child.stdin.take().expect("child stdin is piped");
        let stdout = child.stdout.take().expect("child stdout is piped");
        let (sender, lines) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if sender.send(line).is_err() {
                    break;
                }
            }
        });

        Self {
            child: Some(child),
            stdin: Some(stdin),
            lines,
            next_id: 1,
        }
    }

    fn send(&mut self, message: &Value) {
        let stdin = self.stdin.as_mut().expect("client stdin is open");
        writeln!(stdin, "{message}").expect("write JSON-RPC message");
        stdin.flush().expect("flush JSON-RPC message");
    }

    fn notify(&mut self, method: &str) {
        self.send(&json!({ "jsonrpc": "2.0", "method": method }));
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        self.send(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        }));

        let deadline = Instant::now() + RESPONSE_TIMEOUT;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(
                !remaining.is_zero(),
                "timed out waiting for the `{method}` response"
            );
            let line = self
                .lines
                .recv_timeout(remaining)
                .unwrap_or_else(|error| panic!("no `{method}` response: {error}"));
            let message: Value = serde_json::from_str(&line).unwrap_or_else(|error| {
                panic!("stdout must be protocol JSON, got `{line}`: {error}")
            });
            if message.get("id") == Some(&Value::from(id)) {
                return message;
            }
        }
    }

    fn initialize(&mut self) {
        let response = self.request(
            "initialize",
            json!({
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": { "name": "ascii-forge-tests", "version": "0.1.0" },
            }),
        );
        assert!(
            response.get("error").is_none(),
            "initialize failed: {response}"
        );
        assert!(
            response["result"]["protocolVersion"].is_string(),
            "initialize must negotiate a protocol version: {response}"
        );
        assert_eq!(
            response["result"]["serverInfo"]["name"],
            json!("ascii-forge"),
            "serverInfo must name this crate: {response}"
        );
        assert_eq!(
            response["result"]["serverInfo"]["version"],
            json!(env!("CARGO_PKG_VERSION")),
            "serverInfo must report this crate's version: {response}"
        );
        self.notify("notifications/initialized");
    }

    fn call_tool(&mut self, name: &str, arguments: Value) -> Value {
        let response = self.request(
            "tools/call",
            json!({ "name": name, "arguments": arguments }),
        );
        assert!(
            response.get("error").is_none(),
            "tools/call `{name}` must not be a protocol error: {response}"
        );
        response["result"].clone()
    }

    fn shutdown(mut self) {
        drop(self.stdin.take());
        let Some(mut child) = self.child.take() else {
            return;
        };
        let deadline = Instant::now() + SHUTDOWN_TIMEOUT;
        loop {
            match child.try_wait().expect("wait for the MCP server") {
                Some(_) => return,
                None if Instant::now() >= deadline => break,
                None => thread::sleep(Duration::from_millis(25)),
            }
        }
        let _ = child.kill();
        let _ = child.wait();
    }
}

impl Drop for McpClient {
    fn drop(&mut self) {
        drop(self.stdin.take());
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// Structured tool payload, which every successful tool must return.
fn structured(result: &Value) -> &Value {
    result
        .get("structuredContent")
        .unwrap_or_else(|| panic!("tool result lacks structuredContent: {result}"))
}

/// Write the project config that `create_scene` prompt mode reads.
fn write_config(home: &Path, contents: &str) {
    let path = home.join("ascii-forge").join("config.toml");
    fs::create_dir_all(path.parent().expect("config directory")).expect("create config directory");
    fs::write(path, contents).expect("write config file");
}

/// Create the template scene used by preview and export tests.
fn create_logo_scene(client: &mut McpClient) -> String {
    let result = client.call_tool(
        "create_scene",
        json!({ "template": "logo", "title": "NEBULA", "seed": 7 }),
    );
    assert_eq!(
        result.get("isError"),
        Some(&json!(false)),
        "create_scene failed: {result}"
    );
    structured(&result)["scene_path"]
        .as_str()
        .expect("scene_path is a string")
        .to_string()
}

#[test]
fn tools_list_after_stdio_handshake() {
    let project = TestProject::new();
    let mut client = project.client();
    client.initialize();

    let response = client.request("tools/list", json!({}));
    assert!(
        response.get("error").is_none(),
        "tools/list failed: {response}"
    );
    let tools = response["result"]["tools"]
        .as_array()
        .expect("tools/list returns an array of tools");

    let mut names: Vec<&str> = tools
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    names.sort_unstable();
    let mut expected = TOOL_NAMES.to_vec();
    expected.sort_unstable();
    assert_eq!(names, expected);

    for tool in tools {
        assert!(
            tool["description"]
                .as_str()
                .is_some_and(|text| !text.is_empty()),
            "tool {} must have a description",
            tool["name"]
        );
        assert!(
            tool["inputSchema"].is_object(),
            "tool {} must expose an input schema",
            tool["name"]
        );
    }

    client.shutdown();
}

#[test]
fn create_scene_template_writes_valid_scene_json() {
    let project = TestProject::new();
    let mut client = project.client();
    client.initialize();

    let result = client.call_tool(
        "create_scene",
        json!({ "template": "logo", "title": "NEBULA", "seed": 7 }),
    );
    assert_eq!(result.get("isError"), Some(&json!(false)));
    let payload = structured(&result);

    let scene_path = payload["scene_path"]
        .as_str()
        .expect("scene_path is a string");
    assert!(
        payload.get("absolute_path").is_none(),
        "create_scene must not expose the absolute path: {payload}"
    );
    let relative = Path::new(scene_path);
    assert!(!relative.is_absolute(), "scene_path must be relative");
    assert!(
        scene_path.starts_with(".ascii-forge/artifacts/scenes/"),
        "default scene path must live under .ascii-forge/artifacts/scenes/: {scene_path}"
    );
    assert!(
        scene_path.ends_with("nebula-7.json"),
        "default scene name must combine the title slug and seed: {scene_path}"
    );
    assert_eq!(payload["origin"], json!("template"));
    assert_eq!(payload["template"], json!("logo"));
    assert_eq!(payload["frames"], json!(60));
    assert_eq!(payload["seed"], json!(7));
    assert_eq!(payload["attempts"], json!(0));
    assert_eq!(payload["cached"], json!(false));
    assert!(
        payload.get("usage").is_some(),
        "create_scene must report a usage field (null for templates)"
    );

    let absolute = project.root().join(relative);
    assert!(
        absolute.is_file(),
        "scene file must exist at {}",
        absolute.display()
    );

    let bytes = fs::read(&absolute).expect("read scene file");
    let scene: ascii_forge::scene::Scene =
        serde_json::from_slice(&bytes).expect("scene file is valid Scene JSON");
    scene.validate().expect("written scene passes validation");
    assert_eq!(scene.seed, 7);
    assert_eq!(scene.timeline.frames, 60);

    client.shutdown();
}

#[test]
fn create_scene_rejects_existing_output_unless_overwrite_is_set() {
    let project = TestProject::new();
    let mut client = project.client();
    client.initialize();

    let out = ".ascii-forge/artifacts/scenes/custom.json";
    let first = client.call_tool(
        "create_scene",
        json!({ "template": "logo", "title": "NEBULA", "seed": 1, "out": out }),
    );
    assert_eq!(first.get("isError"), Some(&json!(false)), "{first}");
    assert_eq!(structured(&first)["scene_path"], json!(out));
    let before = fs::read(project.root().join(out)).expect("read first scene");

    let conflict = client.call_tool(
        "create_scene",
        json!({ "template": "logo", "title": "NEBULA", "seed": 1, "out": out }),
    );
    assert_eq!(conflict.get("isError"), Some(&json!(true)), "{conflict}");
    let payload = structured(&conflict);
    assert_eq!(payload["code"], json!("output_exists"));
    assert!(
        payload["message"]
            .as_str()
            .is_some_and(|message| message.contains("overwrite")),
        "conflict must explain the overwrite flag: {payload}"
    );
    assert_eq!(
        fs::read(project.root().join(out)).expect("read scene after conflict"),
        before,
        "a rejected create_scene must not modify the existing scene"
    );

    let replaced = client.call_tool(
        "create_scene",
        json!({
            "template": "logo",
            "title": "NEBULA",
            "seed": 2,
            "out": out,
            "overwrite": true,
        }),
    );
    assert_eq!(replaced.get("isError"), Some(&json!(false)), "{replaced}");
    assert_eq!(structured(&replaced)["seed"], json!(2));
    let after = fs::read(project.root().join(out)).expect("read replaced scene");
    assert_ne!(after, before, "overwrite must replace the scene contents");
    let scene: ascii_forge::scene::Scene =
        serde_json::from_slice(&after).expect("replaced scene is valid Scene JSON");
    assert_eq!(scene.seed, 2);

    client.shutdown();
}

#[test]
fn render_preview_is_bounded_and_reports_frames() {
    let project = TestProject::new();
    let mut client = project.client();
    client.initialize();
    let scene_path = create_logo_scene(&mut client);

    let result = client.call_tool(
        "render_preview",
        json!({
            "scene_path": scene_path,
            "frame": 0,
            "max_cols": 20,
            "max_rows": 10,
        }),
    );
    assert_eq!(result.get("isError"), Some(&json!(false)));
    let payload = structured(&result);

    assert_eq!(payload["frame"], json!(0));
    assert_eq!(payload["frames"], json!(60));
    assert_eq!(payload["cols"], json!(20));
    assert_eq!(payload["rows"], json!(10));
    assert_eq!(payload["truncated"], json!(true));

    let text = payload["preview"].as_str().expect("preview is a string");
    assert!(!text.contains('\u{1b}'), "text preview must not use ANSI");
    let lines: Vec<&str> = text.lines().collect();
    assert!(!lines.is_empty(), "preview must not be empty");
    assert!(lines.len() <= 10, "preview rows must be bounded");
    for line in lines {
        assert!(
            line.chars().count() <= 20,
            "preview line exceeds max_cols: {line:?}"
        );
    }

    let ansi_result = client.call_tool(
        "render_preview",
        json!({
            "scene_path": scene_path,
            "max_cols": 12,
            "max_rows": 4,
            "format": "ansi",
        }),
    );
    let ansi = structured(&ansi_result);
    let ansi_text = ansi["preview"].as_str().expect("ansi preview is a string");
    assert!(
        ansi_text.contains("\u{1b}["),
        "ansi preview uses SGR escapes"
    );
    assert!(
        ansi_text.lines().count() <= 4,
        "ansi preview rows must be bounded"
    );

    client.shutdown();
}

#[test]
fn export_png_writes_decodable_artifact() {
    let project = TestProject::new();
    let mut client = project.client();
    client.initialize();
    let scene_path = create_logo_scene(&mut client);

    let result = client.call_tool(
        "export",
        json!({ "scene_path": scene_path, "format": "png" }),
    );
    assert_eq!(
        result.get("isError"),
        Some(&json!(false)),
        "export failed: {result}"
    );
    let payload = structured(&result);

    let path = payload["path"].as_str().expect("path is a string");
    assert!(
        path.starts_with(".ascii-forge/artifacts/"),
        "default export must live under .ascii-forge/artifacts/, got {path}"
    );
    assert_eq!(payload["mime_type"], json!("image/png"));
    assert_eq!(payload["format"], json!("png"));
    assert_eq!(payload["frames"], json!(1));

    let bytes = fs::read(project.root().join(path)).expect("read exported PNG");
    assert!(!bytes.is_empty(), "exported PNG must not be empty");
    assert_eq!(payload["bytes"].as_u64(), Some(bytes.len() as u64));
    let image = image::load_from_memory(&bytes).expect("decode exported PNG");
    assert!(image.width() > 0 && image.height() > 0);

    let explicit = client.call_tool(
        "export",
        json!({
            "scene_path": scene_path.clone(),
            "format": "png",
            "out": ".ascii-forge/artifacts/exports/custom.png",
        }),
    );
    assert_eq!(explicit.get("isError"), Some(&json!(false)));
    assert_eq!(
        structured(&explicit)["path"],
        json!(".ascii-forge/artifacts/exports/custom.png")
    );

    for out in ["escape.png", "/tmp/escape.png"] {
        let outside = client.call_tool(
            "export",
            json!({ "scene_path": scene_path.clone(), "format": "png", "out": out }),
        );
        assert_eq!(
            outside.get("isError"),
            Some(&json!(true)),
            "export out {out} must be rejected: {outside}"
        );
        assert_eq!(structured(&outside)["code"], json!("unsafe_path"));
    }

    client.shutdown();
}

#[test]
fn export_rejects_existing_artifact_unless_overwrite_is_set() {
    let project = TestProject::new();
    let mut client = project.client();
    client.initialize();
    let scene_path = create_logo_scene(&mut client);

    let first = client.call_tool(
        "export",
        json!({ "scene_path": scene_path, "format": "png" }),
    );
    assert_eq!(first.get("isError"), Some(&json!(false)), "{first}");
    let path = structured(&first)["path"]
        .as_str()
        .expect("path is a string")
        .to_string();
    let before = fs::read(project.root().join(&path)).expect("read first PNG");

    let conflict = client.call_tool(
        "export",
        json!({ "scene_path": scene_path, "format": "png" }),
    );
    assert_eq!(conflict.get("isError"), Some(&json!(true)), "{conflict}");
    let payload = structured(&conflict);
    assert_eq!(payload["code"], json!("output_exists"));
    assert!(
        payload["message"]
            .as_str()
            .is_some_and(|message| message.contains("overwrite")),
        "conflict must explain the overwrite flag: {payload}"
    );
    assert_eq!(
        fs::read(project.root().join(&path)).expect("read PNG after conflict"),
        before,
        "a rejected export must leave the existing artifact untouched"
    );

    let replaced = client.call_tool(
        "export",
        json!({
            "scene_path": scene_path,
            "format": "png",
            "overwrite": true,
        }),
    );
    assert_eq!(replaced.get("isError"), Some(&json!(false)), "{replaced}");
    let replaced_payload = structured(&replaced);
    assert_eq!(replaced_payload["path"], json!(path));
    assert!(
        replaced_payload["bytes"]
            .as_u64()
            .is_some_and(|bytes| bytes > 0),
        "overwritten export must report its size: {replaced_payload}"
    );
    let after = fs::read(project.root().join(&path)).expect("read replaced PNG");
    image::load_from_memory(&after).expect("replaced PNG decodes");

    let exports = project.root().join(".ascii-forge/artifacts/exports");
    let staging: Vec<String> = fs::read_dir(&exports)
        .expect("list exports directory")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with('.'))
        .collect();
    assert!(
        staging.is_empty(),
        "atomic replacement must not leave staging files behind: {staging:?}"
    );

    client.shutdown();
}

#[test]
fn create_scene_prompt_mode_rejects_unsupported_provider_names() {
    let project = TestProject::new();
    write_config(project.home.path(), "[ai]\nprovider = \"anthropic\"\n");
    let mut client = project.client();
    client.initialize();

    let result = client.call_tool("create_scene", json!({ "prompt": "a neon nebula" }));
    assert_eq!(result.get("isError"), Some(&json!(true)), "{result}");
    let payload = structured(&result);
    assert_eq!(payload["code"], json!("provider_error"));
    let message = payload["message"].as_str().expect("message is a string");
    assert!(message.contains("anthropic"), "{message}");
    assert!(message.contains("openai-compatible"), "{message}");
    assert!(
        !message.contains("API key"),
        "an unsupported provider must be rejected before key resolution: {message}"
    );

    client.shutdown();
}

#[test]
fn create_scene_prompt_mode_keeps_supported_provider_names() {
    let project = TestProject::new();
    write_config(
        project.home.path(),
        "[ai]\nprovider = \"openai-compatible\"\n",
    );
    let mut client = project.client();
    client.initialize();

    let result = client.call_tool("create_scene", json!({ "prompt": "a neon nebula" }));
    assert_eq!(result.get("isError"), Some(&json!(true)), "{result}");
    let payload = structured(&result);
    assert_eq!(payload["code"], json!("provider_error"));
    assert!(
        payload["message"]
            .as_str()
            .is_some_and(|message| message.contains("API key")),
        "a supported provider must reach key resolution: {payload}"
    );

    client.shutdown();
}

#[test]
fn invalid_inputs_return_tool_errors_and_keep_serving() {
    let project = TestProject::new();
    let mut client = project.client();
    client.initialize();

    let unknown_template = client.call_tool(
        "create_scene",
        json!({ "template": "unknown-template", "title": "X" }),
    );
    assert_eq!(
        unknown_template.get("isError"),
        Some(&json!(true)),
        "unknown template must be a tool error: {unknown_template}"
    );

    let missing_source = client.call_tool("create_scene", json!({}));
    assert_eq!(missing_source.get("isError"), Some(&json!(true)));
    assert_eq!(structured(&missing_source)["code"], json!("invalid_input"));

    let both_sources = client.call_tool(
        "create_scene",
        json!({ "prompt": "a nebula", "template": "logo", "title": "X" }),
    );
    assert_eq!(both_sources.get("isError"), Some(&json!(true)));
    assert_eq!(structured(&both_sources)["code"], json!("invalid_input"));

    let blank_title = client.call_tool(
        "create_scene",
        json!({ "template": "logo", "title": "   " }),
    );
    assert_eq!(blank_title.get("isError"), Some(&json!(true)));
    assert_eq!(structured(&blank_title)["code"], json!("invalid_input"));

    // No API key is available in the isolated environment, so prompt mode must
    // fail before any network call.
    let prompt_without_key = client.call_tool("create_scene", json!({ "prompt": "a nebula" }));
    assert_eq!(prompt_without_key.get("isError"), Some(&json!(true)));
    let prompt_error = structured(&prompt_without_key);
    assert_eq!(prompt_error["code"], json!("provider_error"));
    assert!(
        prompt_error["message"]
            .as_str()
            .is_some_and(|message| message.contains("API key")),
        "prompt mode must explain the missing key: {prompt_error}"
    );

    let cases = [
        ("missing.json", "scene_not_found"),
        ("/etc/passwd", "unsafe_path"),
        ("../escape.json", "unsafe_path"),
    ];
    for (scene_path, expected_code) in cases {
        let result = client.call_tool("render_preview", json!({ "scene_path": scene_path }));
        assert_eq!(
            result.get("isError"),
            Some(&json!(true)),
            "render_preview({scene_path}) must be a tool error: {result}"
        );
        let payload = structured(&result);
        assert_eq!(payload["code"], json!(expected_code), "{scene_path}");
        assert!(
            payload["message"]
                .as_str()
                .is_some_and(|text| !text.is_empty()),
            "{scene_path} must explain the error"
        );
    }

    let absolute_out = client.call_tool(
        "create_scene",
        json!({ "template": "logo", "title": "X", "out": "/tmp/escape.json" }),
    );
    assert_eq!(absolute_out.get("isError"), Some(&json!(true)));
    assert_eq!(structured(&absolute_out)["code"], json!("unsafe_path"));

    let parent_out = client.call_tool(
        "create_scene",
        json!({ "template": "logo", "title": "X", "out": "../escape.json" }),
    );
    assert_eq!(parent_out.get("isError"), Some(&json!(true)));
    assert_eq!(structured(&parent_out)["code"], json!("unsafe_path"));

    // After every rejected call the server still answers.
    let listed = client.request("tools/list", json!({}));
    assert!(
        listed["result"]["tools"]
            .as_array()
            .is_some_and(|tools| !tools.is_empty()),
        "server must keep serving after tool errors: {listed}"
    );

    client.shutdown();
}

#[test]
fn fonts_and_doctor_return_structured_objects() {
    let project = TestProject::new();
    let mut client = project.client();
    client.initialize();

    let fonts_result = client.call_tool("list_fonts", json!({}));
    assert_eq!(fonts_result.get("isError"), Some(&json!(false)));
    let fonts = structured(&fonts_result);
    assert!(
        fonts["fonts"]
            .as_array()
            .is_some_and(|fonts| !fonts.is_empty()),
        "list_fonts must return the FIGlet catalog: {fonts}"
    );
    assert!(
        fonts.get("font_path").is_some(),
        "list_fonts must report the resolved font path: {fonts}"
    );

    let doctor_result = client.call_tool("doctor", json!({}));
    assert_eq!(doctor_result.get("isError"), Some(&json!(false)));
    let doctor = structured(&doctor_result);
    for key in [
        "ffmpeg",
        "ffprobe",
        "libx264",
        "libvpx_vp9",
        "font",
        "config_dir",
    ] {
        assert!(
            doctor.get(key).is_some_and(Value::is_object),
            "doctor must report `{key}`: {doctor}"
        );
        assert!(
            doctor[key]["status"].is_string(),
            "doctor check `{key}` must carry a status: {doctor}"
        );
    }

    client.shutdown();
}
