//! Project-local cache and artifact paths under `.ascii-forge/`.
//!
//! Cache keys are SHA-256 hex strings over stable request parts. The cache is a
//! convenience, never source-of-truth data: a missing or unreadable entry is a
//! cache miss, and `--no-cache` bypasses it entirely.

use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use sha2::{Digest, Sha256};
use thiserror::Error;

/// Directory created in the project root for caches and generated artifacts.
pub const PROJECT_DIR: &str = ".ascii-forge";

#[derive(Debug, Error)]
pub enum CacheError {
    #[error("invalid cache namespace `{namespace}`")]
    InvalidNamespace { namespace: String },

    #[error("unsafe cache path `{path}`")]
    UnsafePath { path: String },

    #[error("cache io error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

/// Cache rooted at a project's `.ascii-forge` directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectCache {
    root: PathBuf,
}

impl ProjectCache {
    /// Build a cache rooted at an explicit directory.
    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Discover the cache under `<start>/.ascii-forge`.
    pub fn discover(start: &Path) -> Self {
        Self::at(start.join(PROJECT_DIR))
    }

    /// Cache root.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Directory that holds cached responses.
    pub fn cache_dir(&self) -> PathBuf {
        self.root.join("cache")
    }

    /// Directory reserved for generated artifacts.
    pub fn artifact_dir(&self) -> PathBuf {
        self.root.join("artifacts")
    }

    /// Read a cached payload. A missing file is a miss.
    pub fn get(&self, namespace: &str, key: &str) -> Result<Option<Vec<u8>>, CacheError> {
        let path = self.entry_path(namespace, key)?;
        match fs::read(&path) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(source) => Err(CacheError::Io { path, source }),
        }
    }

    /// Write a cached payload, creating parent directories as needed.
    pub fn put(&self, namespace: &str, key: &str, bytes: &[u8]) -> Result<PathBuf, CacheError> {
        let path = self.entry_path(namespace, key)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| CacheError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        fs::write(&path, bytes).map_err(|source| CacheError::Io {
            path: path.clone(),
            source,
        })?;
        Ok(path)
    }

    /// Path for an artifact, rejecting absolute or parent-traversing paths.
    pub fn artifact_path(&self, relative: &Path) -> Result<PathBuf, CacheError> {
        if relative.is_absolute()
            || relative
                .components()
                .any(|component| matches!(component, Component::ParentDir))
        {
            return Err(CacheError::UnsafePath {
                path: relative.display().to_string(),
            });
        }
        Ok(self.artifact_dir().join(relative))
    }

    fn entry_path(&self, namespace: &str, key: &str) -> Result<PathBuf, CacheError> {
        if !is_safe_component(namespace) || !is_safe_component(key) {
            return Err(CacheError::InvalidNamespace {
                namespace: namespace.to_owned(),
            });
        }
        Ok(self.cache_dir().join(namespace).join(format!("{key}.json")))
    }
}

/// Stable SHA-256 hex key over NUL-separated parts.
pub fn sha256_hex(parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part.as_bytes());
        hasher.update([0]);
    }
    format!("{:x}", hasher.finalize())
}

fn is_safe_component(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && !value.contains('/')
        && !value.contains('\\')
}
