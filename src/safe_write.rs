//! Safe-write: the guarantee that a generated file you hand-edit is never
//! silently clobbered (ADR-0004).
//!
//! For each target path we compare three hashes: the lock's recorded hash, the
//! on-disk file's hash, and the freshly rendered content's hash.
//!
//! - file absent                         → write it            → `Created`
//! - on-disk == lock (tool-owned):
//!     - rendered == on-disk             → do nothing          → `Unchanged`
//!     - rendered != on-disk             → overwrite           → `Updated`
//! - on-disk != lock (human-edited / untracked) → refuse, unless `force`.

use std::path::Path;

use crate::audit;
use crate::error::{Error, Result};
use crate::lock::Lockfile;
use crate::paths::safe_join;

/// What [`write`] did to a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteAction {
    Created,
    Updated,
    Unchanged,
}

/// Safe-write `content` to `rel_path` (relative to `project_root`), updating
/// `lock` in memory. Caller is responsible for persisting the lock afterwards.
pub fn write(
    project_root: &Path,
    rel_path: &str,
    content: &str,
    lock: &mut Lockfile,
    force: bool,
) -> Result<WriteAction> {
    let abs = safe_join(project_root, rel_path)?;
    let new_hash = Lockfile::hash(content.as_bytes());

    let action = if abs.exists() {
        let current = std::fs::read(&abs).map_err(|source| Error::Io {
            path: rel_path.to_string(),
            source,
        })?;
        let current_hash = Lockfile::hash(&current);
        let tool_owned = lock.files.get(rel_path) == Some(&current_hash);

        if !tool_owned && !force {
            return Err(Error::Clobber {
                path: rel_path.to_string(),
            });
        }

        if current_hash == new_hash {
            WriteAction::Unchanged
        } else {
            write_file(&abs, rel_path, content)?;
            WriteAction::Updated
        }
    } else {
        write_file(&abs, rel_path, content)?;
        WriteAction::Created
    };

    // The on-disk content now hashes to `new_hash` in every branch
    // (Unchanged means it already did), so recording it is always correct.
    lock.files.insert(rel_path.to_string(), new_hash.clone());

    match action {
        WriteAction::Created => audit::append(project_root, "create", rel_path, &new_hash)?,
        WriteAction::Updated => audit::append(project_root, "update", rel_path, &new_hash)?,
        WriteAction::Unchanged => {}
    }

    Ok(action)
}

fn write_file(abs: &Path, rel: &str, content: &str) -> Result<()> {
    if let Some(parent) = abs.parent() {
        std::fs::create_dir_all(parent).map_err(|source| Error::Io {
            path: rel.to_string(),
            source,
        })?;
    }
    std::fs::write(abs, content).map_err(|source| Error::Io {
        path: rel.to_string(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::AUDIT_FILE;

    fn audit_lines(root: &Path) -> usize {
        std::fs::read_to_string(root.join(AUDIT_FILE))
            .map(|t| t.lines().count())
            .unwrap_or(0)
    }

    #[test]
    fn first_write_creates_file_lock_and_audit() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let mut lock = Lockfile::default();

        let action = write(root, "AGENTS.md", "hello\n", &mut lock, false).unwrap();

        assert_eq!(action, WriteAction::Created);
        assert_eq!(
            std::fs::read_to_string(root.join("AGENTS.md")).unwrap(),
            "hello\n"
        );
        assert!(lock.files.contains_key("AGENTS.md"));
        assert_eq!(audit_lines(root), 1);
    }

    #[test]
    fn second_identical_write_is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        // Run 1
        let mut lock = Lockfile::default();
        write(root, "AGENTS.md", "hello\n", &mut lock, false).unwrap();
        lock.save(root).unwrap();

        // Run 2: lock reloaded from disk, content identical
        let mut lock2 = Lockfile::load(root).unwrap();
        let action = write(root, "AGENTS.md", "hello\n", &mut lock2, false).unwrap();

        assert_eq!(action, WriteAction::Unchanged);
        assert_eq!(audit_lines(root), 1, "no audit entry for a no-op");
    }

    #[test]
    fn refuses_to_clobber_hand_edited_file() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        let mut lock = Lockfile::default();
        write(root, "AGENTS.md", "generated\n", &mut lock, false).unwrap();
        lock.save(root).unwrap();

        // Human edits the generated file.
        std::fs::write(root.join("AGENTS.md"), "human tweak\n").unwrap();

        let mut lock2 = Lockfile::load(root).unwrap();
        let err = write(root, "AGENTS.md", "regenerated\n", &mut lock2, false).unwrap_err();
        assert!(matches!(err, Error::Clobber { .. }));
        // file untouched
        assert_eq!(
            std::fs::read_to_string(root.join("AGENTS.md")).unwrap(),
            "human tweak\n"
        );

        // --force overwrites and re-adopts the file.
        let action = write(root, "AGENTS.md", "regenerated\n", &mut lock2, true).unwrap();
        assert_eq!(action, WriteAction::Updated);
        assert_eq!(
            std::fs::read_to_string(root.join("AGENTS.md")).unwrap(),
            "regenerated\n"
        );
    }
}
