//! Append-only audit trail (`.harness/audit.jsonl`), one JSON line per actual
//! file write. Zero-PII by construction: it records only the repo-relative path,
//! the operation, the content hash, and a Unix timestamp — never a username, an
//! absolute path, or file content.

use std::io::Write;
use std::path::Path;

use serde::Serialize;

use crate::error::{Error, Result};
use crate::lock::HARNESS_DIR;

/// Audit log path relative to the project root.
pub const AUDIT_FILE: &str = ".harness/audit.jsonl";

/// One audit record. `path` is repo-relative; no PII fields exist by design.
#[derive(Debug, Serialize)]
pub struct Entry<'a> {
    pub ts_unix: u64,
    pub op: &'a str,
    pub path: &'a str,
    pub blake3: &'a str,
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Append one record for a write (`op` is `"create"` or `"update"`).
pub fn append(project_root: &Path, op: &str, rel_path: &str, blake3: &str) -> Result<()> {
    let entry = Entry {
        ts_unix: now_unix(),
        op,
        path: rel_path,
        blake3,
    };
    append_entry(project_root, &entry)
}

fn append_entry(project_root: &Path, entry: &Entry<'_>) -> Result<()> {
    let dir = project_root.join(HARNESS_DIR);
    std::fs::create_dir_all(&dir).map_err(|source| Error::Io {
        path: HARNESS_DIR.to_string(),
        source,
    })?;
    let line = serde_json::to_string(entry).map_err(|e| Error::Serialize {
        what: "audit entry".to_string(),
        message: e.to_string(),
    })?;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(project_root.join(AUDIT_FILE))
        .map_err(|source| Error::Io {
            path: AUDIT_FILE.to_string(),
            source,
        })?;
    writeln!(file, "{line}").map_err(|source| Error::Io {
        path: AUDIT_FILE.to_string(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appends_one_json_line_per_call() {
        let tmp = tempfile::tempdir().unwrap();
        append(tmp.path(), "create", "AGENTS.md", "abc").unwrap();
        append(tmp.path(), "update", "AGENTS.md", "def").unwrap();
        let text = std::fs::read_to_string(tmp.path().join(AUDIT_FILE)).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("\"op\":\"create\""));
        assert!(lines[0].contains("\"path\":\"AGENTS.md\""));
        // zero-PII: no absolute path leaked
        assert!(!text.contains("/Users/"));
    }
}
