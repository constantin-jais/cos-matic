//! The out-of-band safe-write sentinel (ADR-0004): `.harness/lock.toml` maps each
//! generated, repo-relative path to the BLAKE3 hash of the content the tool last
//! wrote. It is the source of truth for both clobber protection and drift checks,
//! and is committed to version control.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use miette::NamedSource;
use serde::{Deserialize, Serialize};

use crate::error::{Error, ParseError, Result};

/// Directory holding `aom`'s bookkeeping, relative to the project root.
pub const HARNESS_DIR: &str = ".harness";
/// Lockfile path relative to the project root.
pub const LOCK_FILE: &str = ".harness/lock.toml";

/// The deserialized lockfile. `files` is a `BTreeMap` so serialization is
/// deterministic (sorted keys), which keeps the lockfile diff stable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lockfile {
    pub version: u32,
    #[serde(default)]
    pub files: BTreeMap<String, String>,
}

impl Default for Lockfile {
    fn default() -> Self {
        Self {
            version: 1,
            files: BTreeMap::new(),
        }
    }
}

impl Lockfile {
    /// BLAKE3 hex digest of arbitrary bytes — the canonical content hash.
    pub fn hash(bytes: &[u8]) -> String {
        blake3::hash(bytes).to_hex().to_string()
    }

    fn path(project_root: &Path) -> PathBuf {
        project_root.join(LOCK_FILE)
    }

    /// Load the lockfile, or a fresh empty one if it does not exist yet.
    pub fn load(project_root: &Path) -> Result<Self> {
        let path = Self::path(project_root);
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(source) => {
                return Err(Error::Io {
                    path: LOCK_FILE.to_string(),
                    source,
                });
            }
        };
        toml::from_str(&text).map_err(|e| {
            Error::Parse(Box::new(ParseError {
                name: LOCK_FILE.to_string(),
                message: e.message().to_string(),
                src: NamedSource::new(LOCK_FILE, text.clone()),
                span: e
                    .span()
                    .map(|r| miette::SourceSpan::from((r.start, r.end - r.start))),
            }))
        })
    }

    /// Write the lockfile, creating `.harness/` if needed.
    pub fn save(&self, project_root: &Path) -> Result<()> {
        let dir = project_root.join(HARNESS_DIR);
        std::fs::create_dir_all(&dir).map_err(|source| Error::Io {
            path: HARNESS_DIR.to_string(),
            source,
        })?;
        let text = toml::to_string_pretty(self).expect("lockfile serializes");
        std::fs::write(Self::path(project_root), text).map_err(|source| Error::Io {
            path: LOCK_FILE.to_string(),
            source,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_missing_returns_default() {
        let tmp = tempfile::tempdir().unwrap();
        let lock = Lockfile::load(tmp.path()).unwrap();
        assert_eq!(lock, Lockfile::default());
    }

    #[test]
    fn save_then_load_roundtrips() {
        let tmp = tempfile::tempdir().unwrap();
        let mut lock = Lockfile::default();
        lock.files
            .insert("AGENTS.md".to_string(), Lockfile::hash(b"hello"));
        lock.save(tmp.path()).unwrap();
        let loaded = Lockfile::load(tmp.path()).unwrap();
        assert_eq!(loaded, lock);
    }

    #[test]
    fn hash_is_stable() {
        assert_eq!(Lockfile::hash(b"x"), Lockfile::hash(b"x"));
        assert_ne!(Lockfile::hash(b"x"), Lockfile::hash(b"y"));
    }
}
