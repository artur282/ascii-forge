#![cfg(unix)]

//! Integration tests for the packaging installers and the release pipeline.
//!
//! `install.sh` targets Unix-like systems, so these tests only run on Unix.
//! They never invoke `cargo build`; a fake binary and a temporary `PREFIX`
//! keep the tests fast and offline. The Windows installer and the workflow
//! files are checked structurally because PowerShell is not required.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

const FAKE_BINARY: &str = "#!/bin/sh\necho fake ascii-forge\n";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn repo_file(path: &str) -> String {
    fs::read_to_string(repo_root().join(path))
        .unwrap_or_else(|error| panic!("read {path}: {error}"))
}

fn installed_binary(prefix: &Path) -> PathBuf {
    prefix.join("ascii-forge")
}

fn install_command(args: &[&str]) -> Command {
    let mut command = Command::new("sh");
    command
        .arg(repo_root().join("install.sh"))
        .current_dir(repo_root())
        .args(args)
        .env_remove("PREFIX")
        .env_remove("ASCII_FORGE_BINARY");
    command
}

fn run_install(args: &[&str], envs: &[(&str, &Path)]) -> Output {
    let mut command = install_command(args);
    for (key, value) in envs {
        command.env(key, value);
    }
    command.output().expect("run install.sh")
}

fn make_executable(path: &Path) {
    let mut permissions = fs::metadata(path)
        .expect("read file metadata")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).expect("mark file executable");
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "install.sh failed with {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        stdout(output),
        stderr(output)
    );
}

/// Temporary prefix and fake release binary. Paths include spaces on purpose
/// so every test also pins path quoting.
struct Fixture {
    temp: TempDir,
    prefix: PathBuf,
    binary: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let temp = TempDir::new().expect("create temporary directory");
        let prefix = temp.path().join("local bin");
        let build_dir = temp.path().join("fake build dir");
        fs::create_dir_all(&build_dir).expect("create fake build directory");
        let binary = build_dir.join("ascii-forge");
        fs::write(&binary, FAKE_BINARY).expect("write fake binary");
        make_executable(&binary);
        Self {
            temp,
            prefix,
            binary,
        }
    }

    fn missing_binary(&self) -> PathBuf {
        self.temp.path().join("missing dir").join("ascii-forge")
    }

    fn install(&self) -> Output {
        run_install(
            &["--no-build"],
            &[
                ("PREFIX", self.prefix.as_path()),
                ("ASCII_FORGE_BINARY", self.binary.as_path()),
            ],
        )
    }
}

/// Put a failing `cargo` stub first on `PATH` and return that `PATH` plus the
/// marker file the stub would create. Proves an install never builds.
fn fake_cargo(fixture: &Fixture) -> (String, PathBuf) {
    let bin_dir = fixture.temp.path().join("bin");
    fs::create_dir_all(&bin_dir).expect("create fake PATH directory");
    let marker = fixture.temp.path().join("cargo-was-called");
    let cargo = bin_dir.join("cargo");
    let script = format!("#!/bin/sh\ntouch '{}'\nexit 1\n", marker.display());
    fs::write(&cargo, script).expect("write fake cargo");
    make_executable(&cargo);
    let path = format!(
        "{}:{}",
        bin_dir.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    (path, marker)
}

#[test]
fn no_build_installs_binary_and_reports_dependency() {
    let fixture = Fixture::new();
    let output = fixture.install();
    assert_success(&output);

    let installed = installed_binary(&fixture.prefix);
    assert!(
        installed.is_file(),
        "expected binary at {}",
        installed.display()
    );
    assert_eq!(
        fs::read(&installed).expect("read installed binary"),
        FAKE_BINARY.as_bytes()
    );
    let mode = fs::metadata(&installed)
        .expect("installed binary metadata")
        .permissions()
        .mode();
    assert_ne!(
        mode & 0o111,
        0,
        "installed binary is not executable: mode {mode:o}"
    );

    let stdout = stdout(&output);
    assert!(
        stdout.contains(&installed.display().to_string()),
        "stdout does not report the installed path:\n{stdout}"
    );
    assert!(
        stdout.contains("FFmpeg"),
        "stdout does not mention the FFmpeg runtime dependency:\n{stdout}"
    );
}

#[test]
fn default_mode_installs_existing_binary_without_cargo() {
    let fixture = Fixture::new();
    let (path, cargo_marker) = fake_cargo(&fixture);

    let mut command = install_command(&[]);
    command
        .env("PREFIX", &fixture.prefix)
        .env("ASCII_FORGE_BINARY", &fixture.binary)
        .env("PATH", path);
    let output = command.output().expect("run install.sh");
    assert_success(&output);

    assert!(
        !cargo_marker.exists(),
        "install.sh invoked cargo although an explicit binary was available"
    );
    assert_eq!(
        fs::read(installed_binary(&fixture.prefix)).expect("read installed binary"),
        FAKE_BINARY.as_bytes()
    );
}

#[test]
fn installs_sibling_binary_from_release_archive_layout() {
    let fixture = Fixture::new();
    let archive_dir = fixture.temp.path().join("release archive");
    fs::create_dir_all(&archive_dir).expect("create archive directory");
    fs::copy(
        repo_root().join("install.sh"),
        archive_dir.join("install.sh"),
    )
    .expect("copy install.sh into archive");
    fs::copy(&fixture.binary, archive_dir.join("ascii-forge"))
        .expect("copy fake binary into archive");
    let (path, cargo_marker) = fake_cargo(&fixture);

    let output = Command::new("sh")
        .arg(archive_dir.join("install.sh"))
        .env("PREFIX", &fixture.prefix)
        .env("PATH", path)
        .env_remove("ASCII_FORGE_BINARY")
        .output()
        .expect("run archived install.sh");
    assert_success(&output);

    assert!(
        !cargo_marker.exists(),
        "archive install invoked cargo although the archive ships a binary"
    );
    assert_eq!(
        fs::read(installed_binary(&fixture.prefix)).expect("read installed binary"),
        FAKE_BINARY.as_bytes()
    );
}

#[test]
fn default_prefix_is_home_local_bin() {
    let fixture = Fixture::new();
    let home = fixture.temp.path().join("home");
    fs::create_dir_all(&home).expect("create fake home");

    let mut command = install_command(&["--no-build"]);
    command
        .env("HOME", &home)
        .env("ASCII_FORGE_BINARY", &fixture.binary);
    let output = command.output().expect("run install.sh");
    assert_success(&output);

    let installed = home.join(".local").join("bin").join("ascii-forge");
    assert!(
        installed.is_file(),
        "expected binary at {}",
        installed.display()
    );
}

#[test]
fn prefix_flag_overrides_environment_prefix() {
    let fixture = Fixture::new();
    let environment_prefix = fixture.temp.path().join("environment bin");
    let prefix_arg = fixture.prefix.to_str().expect("utf-8 temporary path");
    let output = run_install(
        &["--no-build", "--prefix", prefix_arg],
        &[
            ("PREFIX", environment_prefix.as_path()),
            ("ASCII_FORGE_BINARY", fixture.binary.as_path()),
        ],
    );
    assert_success(&output);
    assert!(installed_binary(&fixture.prefix).is_file());
    assert!(!installed_binary(&environment_prefix).exists());
}

#[test]
fn repeated_install_is_idempotent() {
    let fixture = Fixture::new();
    assert_success(&fixture.install());
    assert_success(&fixture.install());
    assert_eq!(
        fs::read(installed_binary(&fixture.prefix)).expect("read installed binary"),
        FAKE_BINARY.as_bytes()
    );
}

#[test]
fn no_build_with_missing_binary_fails_actionably() {
    let fixture = Fixture::new();
    let missing = fixture.missing_binary();
    let output = run_install(
        &["--no-build"],
        &[
            ("PREFIX", fixture.prefix.as_path()),
            ("ASCII_FORGE_BINARY", missing.as_path()),
        ],
    );

    assert!(
        !output.status.success(),
        "install.sh should fail when the binary is missing"
    );
    let stderr = stderr(&output);
    assert!(
        stderr.contains("--no-build"),
        "error does not explain --no-build:\n{stderr}"
    );
    assert!(
        stderr.contains("--build"),
        "error does not suggest --build:\n{stderr}"
    );
    assert!(
        stderr.contains("ASCII_FORGE_BINARY"),
        "error does not mention ASCII_FORGE_BINARY:\n{stderr}"
    );
    assert!(
        stderr.contains(&missing.display().to_string()),
        "error does not name the missing binary:\n{stderr}"
    );
    assert!(
        !installed_binary(&fixture.prefix).exists(),
        "a failed install must not leave a binary behind"
    );
}

#[test]
fn uninstall_removes_only_the_installed_binary() {
    let fixture = Fixture::new();
    assert_success(&fixture.install());

    let unrelated = fixture.prefix.join("unrelated.txt");
    fs::write(&unrelated, "keep me").expect("write unrelated file");

    let output = run_install(&["--uninstall"], &[("PREFIX", fixture.prefix.as_path())]);
    assert_success(&output);
    assert!(
        !installed_binary(&fixture.prefix).exists(),
        "uninstall left the binary behind"
    );
    assert!(unrelated.is_file(), "uninstall removed an unrelated file");
    assert_eq!(
        fs::read_to_string(&unrelated).expect("read unrelated file"),
        "keep me"
    );

    let output = run_install(&["--uninstall"], &[("PREFIX", fixture.prefix.as_path())]);
    assert_success(&output);
}

#[test]
fn help_prints_usage_and_succeeds() {
    for flag in ["--help", "-h"] {
        let output = run_install(&[flag], &[]);
        assert_success(&output);
        let stdout = stdout(&output);
        assert!(
            stdout.contains("Usage:"),
            "{flag} output has no usage section:\n{stdout}"
        );
        assert!(
            stdout.contains("--uninstall"),
            "{flag} output does not document --uninstall:\n{stdout}"
        );
    }
}

#[test]
fn install_script_is_executable() {
    let script = repo_root().join("install.sh");
    let mode = fs::metadata(&script)
        .expect("install.sh exists")
        .permissions()
        .mode();
    assert_ne!(mode & 0o111, 0, "install.sh is not executable");
}

#[test]
fn windows_installer_documents_every_supported_flag() {
    let script = repo_file("install.ps1");
    for flag in [
        "-Prefix",
        "-Uninstall",
        "-Build",
        "-NoBuild",
        "-Binary",
        "-Help",
    ] {
        assert!(
            script.contains(flag),
            "install.ps1 does not document {flag}:\n{script}"
        );
    }
    assert!(
        script.contains("Usage:"),
        "install.ps1 has no usage section:\n{script}"
    );
    assert!(
        script.contains("ascii-forge.exe"),
        "install.ps1 does not name the installed executable:\n{script}"
    );
}

#[test]
fn windows_installer_targets_a_user_directory_without_admin_rights() {
    let script = repo_file("install.ps1");
    assert!(
        script.contains("LOCALAPPDATA"),
        "install.ps1 does not default to LOCALAPPDATA:\n{script}"
    );
    assert!(
        script.contains(r"Programs\ascii-forge\bin"),
        "install.ps1 does not document the default install directory:\n{script}"
    );
    for forbidden in ["RunAs", "sudo"] {
        assert!(
            !script.contains(forbidden),
            "install.ps1 must never require administrator rights, found {forbidden}"
        );
    }
}

#[test]
fn release_workflow_builds_locked_release_binaries() {
    let workflow = repo_file(".github/workflows/release.yml");
    assert!(
        workflow.contains("cargo build --release --locked --target ${{ matrix.target }}"),
        "release.yml does not build every target with --locked:\n{workflow}"
    );
}

#[test]
fn release_workflow_runs_a_locked_test_preflight() {
    let workflow = repo_file(".github/workflows/release.yml");
    assert!(
        workflow.contains("cargo test --locked"),
        "release.yml has no locked test preflight:\n{workflow}"
    );
    assert!(
        workflow.contains("needs: preflight"),
        "release builds do not wait for the preflight job:\n{workflow}"
    );
}

#[test]
fn release_workflow_rejects_a_tag_that_does_not_match_cargo_toml() {
    let workflow = repo_file(".github/workflows/release.yml");
    assert!(
        workflow.contains("GITHUB_REF_NAME#v"),
        "release.yml does not strip the v prefix from the tag:\n{workflow}"
    );
    assert!(
        workflow.contains("Cargo.toml"),
        "release.yml does not read the version from Cargo.toml:\n{workflow}"
    );
    assert!(
        workflow.contains("::error::"),
        "release.yml does not fail with a clear annotation on mismatch:\n{workflow}"
    );
}

#[test]
fn release_workflow_serializes_tag_releases() {
    let workflow = repo_file(".github/workflows/release.yml");
    assert!(
        workflow.contains("concurrency:"),
        "release.yml has no concurrency group:\n{workflow}"
    );
    assert!(
        workflow.contains("group: release"),
        "release.yml does not serialize every release:\n{workflow}"
    );
    assert!(
        workflow.contains("cancel-in-progress: false"),
        "release.yml must queue releases instead of cancelling them:\n{workflow}"
    );
    assert_eq!(
        workflow.matches("contents: write").count(),
        1,
        "contents: write must be granted to the publishing job only:\n{workflow}"
    );
}

#[test]
fn release_workflow_ships_the_windows_installer() {
    let workflow = repo_file(".github/workflows/release.yml");
    assert!(
        workflow.contains("install.ps1"),
        "release.yml does not package install.ps1 into the Windows archive:\n{workflow}"
    );
}

#[test]
fn ci_workflow_enforces_dependency_policy() {
    let workflow = repo_file(".github/workflows/ci.yml");
    assert!(
        workflow.contains("cargo deny --locked check"),
        "ci.yml does not run cargo deny:\n{workflow}"
    );
}

#[test]
fn ci_workflow_checks_every_supported_platform() {
    let workflow = repo_file(".github/workflows/ci.yml");
    for runner in ["macos-", "windows-"] {
        assert!(
            workflow.contains(runner),
            "ci.yml has no {runner} job:\n{workflow}"
        );
    }
    assert!(
        workflow.contains("cargo check --locked"),
        "ci.yml does not run cargo check --locked:\n{workflow}"
    );
}

#[test]
fn ci_workflow_checks_the_declared_msrv() {
    let workflow = repo_file(".github/workflows/ci.yml");
    assert!(
        workflow.contains("rust-version"),
        "ci.yml does not read rust-version from Cargo.toml:\n{workflow}"
    );
}
