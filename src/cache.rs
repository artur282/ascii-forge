//! Project-local cache and artifact paths under `.ascii-forge/`.
//!
//! Cache keys are SHA-256 hex strings over stable request parts. The cache is a
//! convenience, never source-of-truth data: a missing or unreadable entry is a
//! cache miss, and `--no-cache` bypasses it entirely.

use std::ffi::OsStr;
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

/// Outcome of [`ProjectCache::clear`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheClear {
    path: PathBuf,
    removed: usize,
}

impl CacheClear {
    /// Cache directory whose entries were removed.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Number of cached files removed, including files in nested namespaces.
    pub fn removed(&self) -> usize {
        self.removed
    }
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

    /// Remove every cached response while preserving artifacts and config.
    ///
    /// The operation refuses to run unless the root is the project-local
    /// `.ascii-forge` directory and the cache directory is a real child of it,
    /// so a symlinked project directory can never delete data outside it.
    pub fn clear(&self) -> Result<CacheClear, CacheError> {
        let cache_dir = self.checked_cache_dir()?;
        let removed = remove_cache_entries(&cache_dir)?;
        Ok(CacheClear {
            path: cache_dir,
            removed,
        })
    }

    fn checked_cache_dir(&self) -> Result<PathBuf, CacheError> {
        if self.root.file_name() != Some(OsStr::new(PROJECT_DIR)) {
            return Err(unsafe_path(&self.root));
        }
        ensure_directory_or_missing(&self.root)?;

        let cache_dir = self.cache_dir();
        if cache_dir.parent() != Some(self.root.as_path()) {
            return Err(unsafe_path(&cache_dir));
        }
        ensure_directory_or_missing(&cache_dir)?;
        Ok(cache_dir)
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

fn remove_cache_entries(cache_dir: &Path) -> Result<usize, CacheError> {
    let entries = match fs::read_dir(cache_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(0),
        Err(source) => {
            return Err(CacheError::Io {
                path: cache_dir.to_path_buf(),
                source,
            });
        }
    };

    let mut removed = 0;
    for entry in entries {
        let entry = entry.map_err(|source| CacheError::Io {
            path: cache_dir.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(|source| CacheError::Io {
            path: path.clone(),
            source,
        })?;
        if file_type.is_dir() {
            removed += count_files(&path)?;
            fs::remove_dir_all(&path).map_err(|source| CacheError::Io { path, source })?;
        } else {
            fs::remove_file(&path).map_err(|source| CacheError::Io { path, source })?;
            removed += 1;
        }
    }
    Ok(removed)
}

fn count_files(dir: &Path) -> Result<usize, CacheError> {
    let entries = fs::read_dir(dir).map_err(|source| CacheError::Io {
        path: dir.to_path_buf(),
        source,
    })?;

    let mut count = 0;
    for entry in entries {
        let entry = entry.map_err(|source| CacheError::Io {
            path: dir.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(|source| CacheError::Io {
            path: path.clone(),
            source,
        })?;
        if file_type.is_dir() {
            count += count_files(&path)?;
        } else {
            count += 1;
        }
    }
    Ok(count)
}

fn ensure_directory_or_missing(path: &Path) -> Result<(), CacheError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(unsafe_path(path)),
        Ok(metadata) if !metadata.is_dir() => Err(unsafe_path(path)),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(CacheError::Io {
            path: path.to_path_buf(),
            source,
        }),
    }
}

fn unsafe_path(path: &Path) -> CacheError {
    CacheError::UnsafePath {
        path: path.display().to_string(),
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
