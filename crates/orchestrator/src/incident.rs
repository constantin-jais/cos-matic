//! Incident model: a structured, fingerprinted record of something that needs
//! attention (a red gate, a CI failure). Fingerprints make issue creation
//! idempotent; the local journal is append-only and zero-PII.

use std::path::{Path, PathBuf};

use serde::Serialize;

/// Visible marker embedded in an issue body so the incident is discoverable.
pub const MARKER_PREFIX: &str = "bolt-cosmatic-fingerprint:";

/// A structured incident. `title`/`body` are human-facing (go to the issue);
/// they are deliberately NOT written to the journal.
#[derive(Debug, Clone)]
pub struct Incident {
    pub fingerprint: String,
    pub kind: String,
    pub severity: String,
    pub title: String,
    pub body: String,
    pub ts_unix: u64,
}

impl Incident {
    /// Build an incident, deriving its fingerprint from `kind` + `key`.
    pub fn new(
        kind: impl Into<String>,
        severity: impl Into<String>,
        title: impl Into<String>,
        body: impl Into<String>,
        key: &str,
        ts_unix: u64,
    ) -> Self {
        let kind = kind.into();
        let fingerprint = fingerprint(&kind, key);
        Self {
            fingerprint,
            kind,
            severity: severity.into(),
            title: title.into(),
            body: body.into(),
            ts_unix,
        }
    }
}

/// Stable content-addressed fingerprint of an incident class (`kind` + `key`).
pub fn fingerprint(kind: &str, key: &str) -> String {
    blake3::hash(format!("{kind}\n{key}").as_bytes())
        .to_hex()
        .to_string()
}

/// Append a visible fingerprint footer to an issue body, so the issue can be
/// found again (idempotency) without relying on labels.
pub fn issue_body_with_marker(body: &str, fingerprint: &str) -> String {
    format!("{body}\n\n<sub>{MARKER_PREFIX} `{fingerprint}`</sub>\n")
}

/// Default journal directory: `~/.bolt-cos-matic`.
pub fn default_journal_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".bolt-cos-matic"))
}

/// What we persist per incident — fingerprint/kind/severity/timestamp only.
/// `title`/`body` are excluded by design (they may carry user content).
#[derive(Serialize)]
struct JournalEntry<'a> {
    ts_unix: u64,
    kind: &'a str,
    severity: &'a str,
    fingerprint: &'a str,
}

/// Append one zero-PII JSON line for `inc` to `<dir>/incidents.jsonl`.
pub fn append_journal(inc: &Incident, dir: &Path) -> std::io::Result<()> {
    use std::io::Write as _;
    std::fs::create_dir_all(dir)?;
    let entry = JournalEntry {
        ts_unix: inc.ts_unix,
        kind: &inc.kind,
        severity: &inc.severity,
        fingerprint: &inc.fingerprint,
    };
    let line = serde_json::to_string(&entry).map_err(std::io::Error::other)?;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("incidents.jsonl"))?;
    writeln!(file, "{line}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_is_stable_and_input_sensitive() {
        assert_eq!(fingerprint("gate-red", "k"), fingerprint("gate-red", "k"));
        assert_ne!(fingerprint("gate-red", "k"), fingerprint("ci-fail", "k"));
        assert_ne!(fingerprint("gate-red", "a"), fingerprint("gate-red", "b"));
    }

    #[test]
    fn incident_new_derives_fingerprint() {
        let inc = Incident::new("gate-red", "high", "T", "B", "key1", 100);
        assert_eq!(inc.fingerprint, fingerprint("gate-red", "key1"));
        assert_eq!(inc.kind, "gate-red");
        assert_eq!(inc.ts_unix, 100);
    }

    #[test]
    fn body_marker_embeds_the_fingerprint() {
        let b = issue_body_with_marker("hello", "abcd1234");
        assert!(b.contains("hello"));
        assert!(b.contains(MARKER_PREFIX));
        assert!(b.contains("abcd1234"));
    }

    #[test]
    fn journal_appends_zero_pii_lines() {
        let tmp = tempfile::tempdir().unwrap();
        let inc = Incident::new("gate-red", "high", "Secret Title", "secret body", "k", 42);
        append_journal(&inc, tmp.path()).unwrap();
        append_journal(&inc, tmp.path()).unwrap();

        let content = std::fs::read_to_string(tmp.path().join("incidents.jsonl")).unwrap();
        let lines: Vec<_> = content.lines().collect();
        assert_eq!(lines.len(), 2, "appended once per call");
        assert!(lines[0].contains("\"fingerprint\""));
        assert!(lines[0].contains("gate-red"));
        // Zero-PII: the human-facing title/body must never reach the journal.
        assert!(!content.contains("Secret Title"));
        assert!(!content.contains("secret body"));
    }
}
