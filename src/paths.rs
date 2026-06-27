//! Lexical path safety: keep includes, content files, and outputs inside the
//! project root, and reject absolute / machine-local paths.
//!
//! Everything here is purely lexical (no filesystem access), so it is valid for
//! files that do not exist yet — which matters because we resolve output paths
//! before writing them.

use std::path::{Component, Path, PathBuf};

use crate::error::{Error, Result};

/// Collapse `.` and `..` components without touching the filesystem.
/// `..` that would pop above the start is preserved as a leading `..`, which the
/// callers below detect as an escape.
fn lexical_normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Join `rel` onto `root`, rejecting absolute paths and any path that escapes
/// `root` via `..`. Used for paths declared relative to the project root
/// (e.g. a target's `output_file`).
pub fn safe_join(root: &Path, rel: &str) -> Result<PathBuf> {
    resolve_within(root, root, rel)
}

/// Resolve `rel` against `base` (the directory of the manifest that declared it)
/// and guarantee the result stays inside `project_root`. Rejects absolute paths
/// and `..` escapes.
pub fn resolve_within(project_root: &Path, base: &Path, rel: &str) -> Result<PathBuf> {
    let rel_path = Path::new(rel);
    if rel_path.is_absolute() {
        return Err(Error::AbsolutePath {
            path: rel.to_string(),
        });
    }

    let joined = lexical_normalize(&base.join(rel_path));
    let root = lexical_normalize(project_root);
    if !joined.starts_with(&root) {
        return Err(Error::EscapingPath {
            path: rel.to_string(),
        });
    }
    Ok(joined)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joins_a_normal_relative_path() {
        let p = safe_join(Path::new("/proj"), "domains/x.md").unwrap();
        assert_eq!(p, PathBuf::from("/proj/domains/x.md"));
    }

    #[test]
    fn allows_dotdot_that_stays_within_root() {
        let p = safe_join(Path::new("/proj"), "a/../b.md").unwrap();
        assert_eq!(p, PathBuf::from("/proj/b.md"));
    }

    #[test]
    fn rejects_escaping_path() {
        let err = safe_join(Path::new("/proj"), "../secrets").unwrap_err();
        assert!(matches!(err, Error::EscapingPath { .. }));
    }

    #[test]
    fn rejects_absolute_path() {
        let err = safe_join(Path::new("/proj"), "/etc/passwd").unwrap_err();
        assert!(matches!(err, Error::AbsolutePath { .. }));
    }

    #[test]
    fn resolves_relative_to_base_dir() {
        let p = resolve_within(Path::new("/proj"), Path::new("/proj/shared"), "foo.md").unwrap();
        assert_eq!(p, PathBuf::from("/proj/shared/foo.md"));
    }

    #[test]
    fn base_relative_dotdot_within_root_is_ok() {
        let p = resolve_within(Path::new("/proj"), Path::new("/proj/shared"), "../top.md").unwrap();
        assert_eq!(p, PathBuf::from("/proj/top.md"));
    }

    #[test]
    fn base_relative_dotdot_escaping_root_is_rejected() {
        let err =
            resolve_within(Path::new("/proj"), Path::new("/proj/shared"), "../../etc").unwrap_err();
        assert!(matches!(err, Error::EscapingPath { .. }));
    }
}
