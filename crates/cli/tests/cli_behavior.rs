//! Black-box behavioral tests on the `aom` binary itself.
//!
//! The library-level e2e tests in `crates/aom` exercise `generate::run`, but the
//! binary's *observable output* — the per-file report, the graceful-degradation
//! warnings on stderr, and the up-to-date message — went untested. The workspace
//! split relocated `run()` into this crate and nearly dropped the warnings loop
//! and reverted the "file(s)" wording; nothing was red. These tests pin that
//! surface so such a regression fails loudly.

use std::fs;
use std::process::Command;

/// Path to the compiled `aom` binary, injected by Cargo for integration tests of
/// the crate that defines the bin.
const AOM: &str = env!("CARGO_BIN_EXE_aom");

fn repo_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

#[test]
fn check_prints_files_up_to_date_message() {
    // The committed example is in sync, so `--check` must exit 0 and print the
    // file-count message. Pins the "N file(s) up to date" wording — a refactor
    // once reverted it to "target(s)".
    let example = repo_root().join("examples/minimal");
    let out = Command::new(AOM)
        .args(["generate", "--check"])
        .current_dir(&example)
        .output()
        .expect("spawn aom generate --check");

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "expected exit 0, got {:?}\nstdout:\n{stdout}\nstderr:\n{stderr}",
        out.status
    );
    assert!(
        stdout.contains("file(s) up to date"),
        "missing up-to-date message; stdout was:\n{stdout}"
    );
    // The example declares multiple targets; the report lists each output file.
    assert!(
        stdout.contains("AGENTS.md") && stdout.contains("CLAUDE.md"),
        "report should list every generated file; stdout:\n{stdout}"
    );
}

#[test]
fn degradation_warnings_are_printed_to_stderr() {
    // A glob-scoped domain rendered to `universal` (which cannot express glob
    // activation) must surface a warning on stderr — the binary's warnings loop.
    // Pins the loop a refactor nearly dropped. Generation still succeeds:
    // degradation is a warning, not an error.
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    fs::write(
        root.join("harness.toml"),
        r#"
[package]
name = "warns"

[[domains]]
name = "rust"
priority = 10
content = "RUST"
globs = ["src/**/*.rs"]

[[profiles]]
name = "default"
domains = ["rust"]

[[targets]]
name = "agents"
adapter = "universal"
output_file = "AGENTS.md"
profile = "default"
"#,
    )
    .unwrap();

    let out = Command::new(AOM)
        .arg("generate")
        .current_dir(root)
        .output()
        .expect("spawn aom generate");

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "expected exit 0; stderr:\n{stderr}\nstdout:\n{}",
        String::from_utf8_lossy(&out.stdout)
    );
    assert!(
        stderr.contains("warning:"),
        "expected a degradation warning on stderr; stderr was:\n{stderr}"
    );
    assert!(
        stderr.contains("glob"),
        "warning should name the unsupported glob activation; stderr:\n{stderr}"
    );
}
