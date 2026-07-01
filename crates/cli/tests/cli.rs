//! CLI-level tests: exercise the actual `bolt-cosmatic` binary and assert exit codes (the
//! contract a CI pipeline depends on). The library tests cover behavior; these
//! cover the process boundary.

use std::fs;
use std::path::Path;
use std::process::Command;

fn bolt_cosmatic() -> Command {
    Command::new(env!("CARGO_BIN_EXE_bolt-cosmatic"))
}

/// A project whose `orphan` domain is unused, with a `no-dead-domains` goal of
/// the given kind.
fn write_project(dir: &Path, gate_kind: &str) {
    fs::write(
        dir.join("harness.toml"),
        format!(
            r#"
[package]
name = "cli-test"
[[domains]]
name = "used"
content = "U"
[[domains]]
name = "orphan"
content = "O"
[[profiles]]
name = "default"
domains = ["used"]
[[targets]]
name = "agents"
adapter = "universal"
output_file = "AGENTS.md"
profile = "default"
[[goals]]
kind = "{gate_kind}"
check = "no-dead-domains"
"#
        ),
    )
    .unwrap();
}

#[test]
fn goals_command_exits_zero_when_nothing_blocks() {
    let tmp = tempfile::tempdir().unwrap();
    // observability never blocks, even though `orphan` is dead.
    write_project(tmp.path(), "observability");
    let status = bolt_cosmatic()
        .args(["goals", "--manifest"])
        .arg(tmp.path().join("harness.toml"))
        .status()
        .unwrap();
    assert!(status.success());
}

#[test]
fn goals_command_exits_nonzero_on_hard_gate_failure() {
    let tmp = tempfile::tempdir().unwrap();
    write_project(tmp.path(), "hard_gate");
    let status = bolt_cosmatic()
        .args(["goals", "--manifest"])
        .arg(tmp.path().join("harness.toml"))
        .status()
        .unwrap();
    assert!(!status.success());
}

#[test]
fn generate_exits_nonzero_and_writes_nothing_on_hard_gate_failure() {
    let tmp = tempfile::tempdir().unwrap();
    write_project(tmp.path(), "hard_gate");
    let status = bolt_cosmatic()
        .args(["generate", "--manifest"])
        .arg(tmp.path().join("harness.toml"))
        .status()
        .unwrap();
    assert!(!status.success());
    assert!(!tmp.path().join("AGENTS.md").exists());
}
