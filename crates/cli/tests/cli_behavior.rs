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

// --- Orchestrator safety envelope, end-to-end through the binary ---
//
// The kill-switch is the envelope's most important guarantee: with it set, a
// command must refuse BEFORE any network or agent call. These run hermetically
// (no gh, no git remote, no claude) — an explicit `--repo` skips remote
// resolution, and the kill-switch short-circuits ahead of every side effect. They
// cover the orchestrator arms of `main.rs`, which no library test reaches.

fn run_killed(
    args: &[&str],
    disable_var: &str,
    extra_env: &[(&str, &str)],
) -> std::process::Output {
    let tmp = tempfile::tempdir().unwrap();
    let mut c = Command::new(AOM);
    c.args(args).env(disable_var, "1").current_dir(tmp.path());
    for (k, v) in extra_env {
        c.env(k, v);
    }
    c.output().expect("spawn aom")
}

fn assert_refused(out: &std::process::Output, who: &str) {
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        out.status.code(),
        Some(2),
        "{who}: expected exit 2 (refused); stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("refused"),
        "{who}: expected 'refused' on stderr; got:\n{stderr}"
    );
}

#[test]
fn dispatch_kill_switch_refuses_before_any_agent() {
    let out = run_killed(
        &["dispatch", "--issue", "1", "--title", "x", "--repo", "o/n"],
        "AOM_DISPATCH_DISABLED",
        &[],
    );
    assert_refused(&out, "dispatch");
}

#[test]
fn automerge_kill_switch_refuses_before_any_merge() {
    // The automerge arm builds the GitHub forge (which reads a token) before the
    // envelope, so a token must be present; the kill-switch then refuses before
    // any API call.
    let out = run_killed(
        &["automerge", "--branch", "x", "--repo", "o/n"],
        "AOM_AUTOMERGE_DISABLED",
        &[("GITHUB_TOKEN", "ghp_test_dummy")],
    );
    assert_refused(&out, "automerge");
}

#[test]
fn deploy_kill_switch_refuses_before_any_command() {
    // The deploy arm reads its commands before the envelope, so they must be set;
    // the kill-switch then refuses before any of them runs.
    let out = run_killed(
        &["deploy", "--target", "x", "--repo", "o/n"],
        "AOM_DEPLOY_DISABLED",
        &[
            ("AOM_DEPLOY_CANARY", "true"),
            ("AOM_DEPLOY_PROMOTE", "true"),
            ("AOM_DEPLOY_ROLLBACK", "true"),
            ("AOM_DEPLOY_SMOKE", "true"),
        ],
    );
    assert_refused(&out, "deploy");
}

#[test]
fn loop_kill_switch_refuses_before_any_stage() {
    // The loop arm builds the GitHub forge (which reads a token) before the
    // envelope, so a token must be present; the kill-switch then refuses before
    // any stage runs.
    let out = run_killed(
        &["loop", "--issue", "1", "--title", "x", "--repo", "o/n"],
        "AOM_LOOP_DISABLED",
        &[("GITHUB_TOKEN", "ghp_test_dummy")],
    );
    assert_refused(&out, "loop");
}
