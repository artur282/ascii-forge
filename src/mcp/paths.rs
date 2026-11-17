//! Project-relative path resolution and scene loading for MCP tools.
//!
//! MCP clients never name absolute paths. Scene paths are canonicalized and
//! must stay under the current working directory; output paths are checked
//! through their nearest existing ancestor so a symlink cannot redirect a
//! write outside the project.

use std::fs;
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};

use crate::cache::PROJECT_DIR;
use crate::scene::Scene;

use super::error::ToolError;

/// Directory (under the project root) that holds every generated artifact.
pub const ARTIFACTS_DIR: &str = "artifacts";
/// Directory for default scene files.
pub const SCENES_DIR: &str = "scenes";
/// Directory for default export files.
pub const EXPORTS_DIR: &str = "exports";
/// Upper bound for a title-derived file name.
const MAX_SLUG_CHARS: usize = 48;

/// Current working directory, which is the project root for every tool call.
pub fn current_dir() -> Result<PathBuf, ToolError> {
    std::env::current_dir()
        .map_err(|error| ToolError::io(format!("could not resolve the current directory: {error}")))
}

/// Normalize a caller-supplied relative path.
///
/// Absolute paths, parent traversal and empty paths are rejected. `.`
/// components are dropped, which is how a path like `./out.json` becomes
/// `out.json`.
pub fn safe_relative(raw: &str, field: &str) -> Result<PathBuf, ToolError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(ToolError::invalid_input(format!(
            "{field} must not be empty"
        )));
    }

    let path = Path::new(trimmed);
    if path.is_absolute() {
        return Err(ToolError::unsafe_path(format!(
            "{field} must be relative, got `{trimmed}`"
        )));
    }

    let mut relative = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => relative.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(ToolError::unsafe_path(format!(
                    "{field} must stay inside the project directory, got `{trimmed}`"
                )));
            }
        }
    }

    if relative.as_os_str().is_empty() {
        return Err(ToolError::invalid_input(format!(
            "{field} must name a file, got `{trimmed}`"
        )));
    }
    Ok(relative)
}

/// Resolve an output path inside the project, creating nothing yet.
///
/// The nearest existing ancestor is canonicalized and must stay under the
/// project root, and an existing target is canonicalized too, so symlinked
/// directories and files cannot escape.
pub fn prepare_output_path(cwd: &Path, relative: &Path) -> Result<PathBuf, ToolError> {
    let root = canonical_root(cwd)?;
    let candidate = root.join(relative);
    let Some(parent) = candidate.parent() else {
        return Err(ToolError::unsafe_path(format!(
            "output path `{}` has no parent directory",
            relative.display()
        )));
    };

    let ancestor = canonical_existing_ancestor(parent)?;
    if !ancestor.starts_with(&root) {
        return Err(ToolError::unsafe_path(format!(
            "output path `{}` escapes the project directory",
            relative.display()
        )));
    }

    verify_existing_target(&candidate, &root, relative)?;
    Ok(candidate)
}

/// Require an export path to live under `.ascii-forge/artifacts/`.
pub fn require_artifacts_prefix(relative: &Path) -> Result<(), ToolError> {
    if is_under_artifacts(relative) {
        Ok(())
    } else {
        Err(ToolError::unsafe_path(format!(
            "out must stay under {PROJECT_DIR}/{ARTIFACTS_DIR}/, got `{}`",
            relative.display()
        )))
    }
}

/// Default scene path: `.ascii-forge/artifacts/scenes/<slug>-<seed>.json`.
pub fn default_scene_relative(title: &str, seed: u64) -> PathBuf {
    PathBuf::from(PROJECT_DIR)
        .join(ARTIFACTS_DIR)
        .join(SCENES_DIR)
        .join(format!("{}-{seed}.json", slugify(title)))
}

/// Default export path: `.ascii-forge/artifacts/exports/<stem>.<ext>`.
pub fn default_export_relative(scene_path: &Path, extension: &str) -> PathBuf {
    let stem = scene_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.is_empty())
        .unwrap_or("scene");
    PathBuf::from(PROJECT_DIR)
        .join(ARTIFACTS_DIR)
        .join(EXPORTS_DIR)
        .join(format!("{stem}.{extension}"))
}

/// Create the missing parent directories of an output path.
pub fn ensure_parent(path: &Path) -> Result<(), ToolError> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(|error| {
            ToolError::io(format!(
                "could not create directory {}: {error}",
                parent.display()
            ))
        })?;
    }
    Ok(())
}

/// Read, parse and validate a scene named relative to the project root.
pub fn load_scene(cwd: &Path, raw: &str) -> Result<(PathBuf, Scene), ToolError> {
    let root = canonical_root(cwd)?;
    let relative = safe_relative(raw, "scene_path")?;
    let candidate = root.join(&relative);
    let resolved = candidate
        .canonicalize()
        .map_err(|error| match error.kind() {
            ErrorKind::NotFound => {
                ToolError::scene_not_found(format!("scene `{raw}` was not found in the project"))
            }
            _ => ToolError::io(format!("could not resolve scene `{raw}`: {error}")),
        })?;

    if !resolved.starts_with(&root) {
        return Err(ToolError::unsafe_path(format!(
            "scene `{raw}` resolves outside the project directory"
        )));
    }
    if !resolved.is_file() {
        return Err(ToolError::scene_not_found(format!(
            "scene `{raw}` is not a file"
        )));
    }

    let contents = fs::read_to_string(&resolved)
        .map_err(|error| ToolError::io(format!("could not read scene `{raw}`: {error}")))?;
    let scene: Scene = serde_json::from_str(&contents).map_err(|error| {
        ToolError::invalid_scene(format!("scene `{raw}` is not valid Scene JSON: {error}"))
    })?;
    scene.validate().map_err(|errors| {
        let mut message = format!("scene `{raw}` failed validation:");
        for error in errors {
            message.push_str(&format!("\n  - {error}"));
        }
        ToolError::invalid_scene(message)
    })?;

    Ok((resolved, scene))
}

/// Project-relative, forward-slash display form of an absolute path.
pub fn relative_display(cwd: &Path, path: &Path) -> Result<String, ToolError> {
    let root = canonical_root(cwd)?;
    let relative = path.strip_prefix(&root).map_err(|_| {
        ToolError::unsafe_path(format!(
            "path {} is outside the project directory",
            path.display()
        ))
    })?;
    let parts: Vec<String> = relative
        .components()
        .filter_map(|component| match component {
            Component::Normal(part) => Some(part.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect();
    if parts.is_empty() {
        return Err(ToolError::invalid_input("path must name a file"));
    }
    Ok(parts.join("/"))
}

fn canonical_root(cwd: &Path) -> Result<PathBuf, ToolError> {
    cwd.canonicalize().map_err(|error| {
        ToolError::io(format!(
            "could not resolve the project directory {}: {error}",
            cwd.display()
        ))
    })
}

fn canonical_existing_ancestor(path: &Path) -> Result<PathBuf, ToolError> {
    let mut current = path.to_path_buf();
    loop {
        match current.canonicalize() {
            Ok(resolved) => return Ok(resolved),
            Err(error) if error.kind() == ErrorKind::NotFound => {
                if !current.pop() {
                    return Err(ToolError::io(
                        "could not find an existing project directory".to_string(),
                    ));
                }
            }
            Err(error) => {
                return Err(ToolError::io(format!(
                    "could not resolve {}: {error}",
                    current.display()
                )));
            }
        }
    }
}

fn verify_existing_target(candidate: &Path, root: &Path, relative: &Path) -> Result<(), ToolError> {
    match fs::symlink_metadata(candidate) {
        Ok(_) => {
            let resolved = candidate.canonicalize().map_err(|error| {
                ToolError::io(format!(
                    "could not resolve output path `{}`: {error}",
                    relative.display()
                ))
            })?;
            if !resolved.starts_with(root) {
                return Err(ToolError::unsafe_path(format!(
                    "output path `{}` resolves outside the project directory",
                    relative.display()
                )));
            }
            if resolved.is_dir() {
                return Err(ToolError::invalid_input(format!(
                    "output path `{}` is a directory",
                    relative.display()
                )));
            }
            Ok(())
        }
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(ToolError::io(format!(
            "could not inspect output path `{}`: {error}",
            relative.display()
        ))),
    }
}

fn is_under_artifacts(relative: &Path) -> bool {
    let mut parts = relative
        .components()
        .filter_map(|component| match component {
            Component::Normal(part) => part.to_str(),
            _ => None,
        });
    parts.next() == Some(PROJECT_DIR)
        && parts.next() == Some(ARTIFACTS_DIR)
        && parts.next().is_some()
}

/// Lowercase, dash-separated file name stem derived from a title.
fn slugify(title: &str) -> String {
    let mut slug = String::with_capacity(title.len());
    let mut pending_dash = false;
    for character in title.chars() {
        if character.is_ascii_alphanumeric() {
            if pending_dash && !slug.is_empty() {
                slug.push('-');
            }
            pending_dash = false;
            slug.push(character.to_ascii_lowercase());
            if slug.len() >= MAX_SLUG_CHARS {
                break;
            }
        } else {
            pending_dash = true;
        }
    }
    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        "scene".to_string()
    } else {
        slug.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_produces_safe_names() {
        assert_eq!(slugify("NEBULA"), "nebula");
        assert_eq!(slugify("Hello, World!"), "hello-world");
        assert_eq!(slugify("  --  "), "scene");
        assert_eq!(slugify("ASCII Forge 2026"), "ascii-forge-2026");
        assert!(slugify(&"x".repeat(200)).len() <= MAX_SLUG_CHARS);
    }

    #[test]
    fn safe_relative_rejects_unsafe_paths() {
        assert_eq!(
            safe_relative("scenes/logo.json", "out").unwrap(),
            PathBuf::from("scenes/logo.json")
        );
        assert_eq!(
            safe_relative("./scene.json", "out").unwrap(),
            PathBuf::from("scene.json")
        );
        assert!(safe_relative("", "out").is_err());
        assert!(safe_relative("/tmp/scene.json", "out").is_err());
        assert!(safe_relative("../scene.json", "out").is_err());
        assert!(safe_relative("scenes/../../escape.json", "out").is_err());
    }

    #[test]
    fn artifacts_prefix_is_required_for_exports() {
        assert!(
            require_artifacts_prefix(Path::new(".ascii-forge/artifacts/exports/a.png")).is_ok()
        );
        assert!(require_artifacts_prefix(Path::new(".ascii-forge/artifacts/a.png")).is_ok());
        assert!(require_artifacts_prefix(Path::new(".ascii-forge/artifacts")).is_err());
        assert!(require_artifacts_prefix(Path::new("exports/a.png")).is_err());
        assert!(require_artifacts_prefix(Path::new(".ascii-forge/scenes/a.json")).is_err());
    }

    #[test]
    fn scene_paths_cannot_escape_through_symlinks() {
        let project = tempfile::tempdir().expect("temp project");
        let outside = tempfile::tempdir().expect("temp outside");
        let scene = json_scene();
        fs::write(outside.path().join("scene.json"), &scene).expect("write scene");

        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(outside.path(), project.path().join("link"))
                .expect("create symlink");
            let error = load_scene(project.path(), "link/scene.json").expect_err("symlink escapes");
            assert_eq!(error.code(), "unsafe_path");
        }

        fs::write(project.path().join("scene.json"), &scene).expect("write in-project scene");
        let (path, loaded) = load_scene(project.path(), "scene.json").expect("load scene");
        assert!(path.starts_with(project.path().canonicalize().unwrap()));
        assert_eq!(loaded.timeline.frames, 1);
    }

    fn json_scene() -> String {
        serde_json::to_string(&crate::scene::Scene {
            version: crate::scene::SCENE_VERSION,
            seed: 0,
            canvas: crate::scene::Canvas {
                cols: 2,
                rows: 1,
                cell_aspect: 0.5,
                background: crate::scene::Rgba::rgb(0, 0, 0),
                foreground: crate::scene::Rgba::rgb(255, 255, 255),
            },
            layers: Vec::new(),
            timeline: crate::scene::Timeline {
                fps: 30,
                frames: 1,
                loop_: false,
            },
            effects: Vec::new(),
            output: crate::scene::OutputSpec::default(),
        })
        .expect("serialize scene")
    }
}
