//! Consistency gates for the ADR set, run as part of `cargo test --workspace`
//! so they double as a CI gate (and are runnable as-is by the orchestrator's
//! Cargo-based gate runner). They enforce the conventions that this project's
//! concurrent branches kept breaking by hand:
//!
//!   * ADR numbers (filename prefixes) are unique — concurrent branches must not
//!     ship two ADRs under the same number.
//!   * Cross-references use the stable slug, never the number, so renumbering an
//!     ADR never has to touch code. No `.rs` file may reference an ADR by number;
//!     it must write `ADR: <slug>` instead.
//!   * Every `ADR: <slug>` resolves to an existing ADR file.
//!   * The README index and the ADR files are in bijection (none missing, none
//!     dangling).
//!   * Each ADR's title number matches its filename number.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn adr_dir() -> PathBuf {
    repo_root().join("docs/adr")
}

/// `(number, slug, path)` for every `NNNN-slug.md` ADR file (README excluded).
fn adr_files() -> Vec<(String, String, PathBuf)> {
    let mut out = Vec::new();
    for entry in fs::read_dir(adr_dir()).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if name == "README.md" || !name.ends_with(".md") {
            continue;
        }
        let stem = name.trim_end_matches(".md");
        if let Some((num, slug)) = stem.split_once('-')
            && num.len() == 4
            && num.bytes().all(|b| b.is_ascii_digit())
        {
            out.push((num.to_string(), slug.to_string(), path));
        }
    }
    out.sort();
    out
}

/// Collect files with the given extension under `dir`, skipping `target/`.
fn collect(dir: &Path, ext: &str, acc: &mut Vec<PathBuf>) {
    if !dir.exists() {
        return;
    }
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n == "target") {
                continue;
            }
            collect(&path, ext, acc);
        } else if path.extension().is_some_and(|e| e == ext) {
            acc.push(path);
        }
    }
}

/// Slugs referenced as `ADR: <slug>` in `text`.
fn slug_refs(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (i, _) in text.match_indices("ADR: ") {
        let slug: String = text[i + 5..]
            .chars()
            .take_while(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '-')
            .collect();
        if !slug.is_empty() {
            out.push(slug);
        }
    }
    out
}

/// Numeric references of the form `ADR-NNNN` in `text`.
fn numeric_refs(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (i, _) in text.match_indices("ADR-") {
        let digits: String = text[i + 4..].chars().take(4).collect();
        if digits.len() == 4 && digits.bytes().all(|b| b.is_ascii_digit()) {
            out.push(format!("ADR-{digits}"));
        }
    }
    out
}

/// Link targets `](target)` in markdown `text`.
fn md_link_targets(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (i, _) in text.match_indices("](") {
        let target: String = text[i + 2..].chars().take_while(|c| *c != ')').collect();
        out.push(target);
    }
    out
}

fn rel(path: &Path) -> String {
    path.strip_prefix(repo_root())
        .unwrap_or(path)
        .display()
        .to_string()
}

#[test]
fn adr_set_is_consistent() {
    let files = adr_files();
    assert!(
        !files.is_empty(),
        "no ADR files found under {}",
        adr_dir().display()
    );

    let slugs: BTreeSet<&str> = files.iter().map(|(_, s, _)| s.as_str()).collect();
    let mut violations: Vec<String> = Vec::new();

    // g1 — numbers are unique.
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for (num, _, _) in &files {
        *counts.entry(num).or_default() += 1;
    }
    for (num, n) in &counts {
        if *n > 1 {
            violations.push(format!("ADR number {num} is used by {n} files"));
        }
    }

    // g5 — each title number matches its filename number.
    for (num, slug, path) in &files {
        let first = fs::read_to_string(path).unwrap();
        let first = first.lines().next().unwrap_or("");
        if !first.starts_with(&format!("# ADR-{num}")) {
            violations.push(format!(
                "{slug}: title `{first}` does not start with `# ADR-{num}`"
            ));
        }
    }

    // Gather references. `.rs` files: numbers are forbidden (slug only). ADR
    // bodies and `.rs` files: every slug must resolve.
    let mut rs_files = Vec::new();
    collect(&repo_root().join("crates"), "rs", &mut rs_files);

    for path in &rs_files {
        let text = fs::read_to_string(path).unwrap();
        for numeric in numeric_refs(&text) {
            violations.push(format!(
                "{}: numeric `{numeric}` — reference ADRs by slug (`ADR: <slug>`) so renumbering never touches code",
                rel(path)
            ));
        }
    }

    let mut ref_sources = rs_files.clone();
    ref_sources.extend(files.iter().map(|(_, _, p)| p.clone()));
    for path in &ref_sources {
        let text = fs::read_to_string(path).unwrap();
        for slug in slug_refs(&text) {
            if !slugs.contains(slug.as_str()) {
                violations.push(format!(
                    "{}: dangling `ADR: {slug}` (no such ADR)",
                    rel(path)
                ));
            }
        }
    }

    // g3 — README index and ADR files are in bijection.
    let readme = fs::read_to_string(adr_dir().join("README.md")).unwrap();
    for (num, slug, _) in &files {
        let link = format!("{num}-{slug}.md");
        if !readme.contains(&link) {
            violations.push(format!("README is missing an index row for `{link}`"));
        }
    }
    for target in md_link_targets(&readme) {
        let b = target.as_bytes();
        let looks_like_adr = b.len() > 5
            && b[..4].iter().all(u8::is_ascii_digit)
            && b[4] == b'-'
            && target.ends_with(".md");
        if looks_like_adr && !adr_dir().join(&target).exists() {
            violations.push(format!("README links `{target}`, which does not exist"));
        }
    }

    assert!(
        violations.is_empty(),
        "ADR consistency violations:\n  - {}",
        violations.join("\n  - ")
    );
}
