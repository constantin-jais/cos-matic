use std::fs;
use std::process::Command;

fn bolt_cosmatic() -> Command {
    Command::new(env!("CARGO_BIN_EXE_bolt-cosmatic"))
}

#[test]
fn stack_detect_json_runs_without_network_or_secrets() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(
        tmp.path().join("Cargo.toml"),
        r#"
[package]
name = "demo"
version = "0.1.0"
edition = "2024"
"#,
    )
    .unwrap();

    let output = bolt_cosmatic()
        .args(["stack", "detect", "--root"])
        .arg(tmp.path())
        .arg("--json")
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("stack_detect"));
    assert!(stdout.contains("local_only"));
}

#[test]
fn dependency_audit_exits_nonzero_for_forbidden_provider() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(
        tmp.path().join("package.json"),
        r#"{"dependencies":{"@aws-sdk/client-s3":"latest"}}"#,
    )
    .unwrap();

    let status = bolt_cosmatic()
        .args(["stack", "dependency-audit", "--root"])
        .arg(tmp.path())
        .status()
        .unwrap();

    assert!(!status.success());
}

#[test]
fn local_smoke_refuses_deploy_like_commands() {
    let tmp = tempfile::tempdir().unwrap();

    let status = bolt_cosmatic()
        .args(["stack", "local-smoke", "--root"])
        .arg(tmp.path())
        .args(["--cmd", "clever deploy"])
        .status()
        .unwrap();

    assert!(!status.success());
}
