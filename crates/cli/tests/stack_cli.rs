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

#[test]
fn db_security_check_json_runs_without_db_connection() {
    let tmp = tempfile::tempdir().unwrap();
    fs::create_dir(tmp.path().join("fixtures")).unwrap();
    fs::write(
        tmp.path().join("fixtures/safe.sql"),
        "create table items (id uuid primary key);",
    )
    .unwrap();

    let output = bolt_cosmatic()
        .args(["stack", "db_security_check", "--root"])
        .arg(tmp.path())
        .arg("--json")
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("db_security_check"));
    assert!(stdout.contains("local_only"));
}

#[test]
fn db_security_check_refuses_database_url_by_default() {
    let tmp = tempfile::tempdir().unwrap();

    let output = bolt_cosmatic()
        .args(["stack", "db_security_check", "--root"])
        .arg(tmp.path())
        .args([
            "--database-url",
            "postgres://user:secret@example.invalid/db",
        ])
        .arg("--json")
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("refused_db_connection"));
    assert!(!stdout.contains("secret@example"));
}

#[test]
fn adr_generate_outputs_markdown_draft() {
    let output = bolt_cosmatic()
        .args([
            "stack",
            "adr_generate",
            "--title",
            "Local DB checks",
            "--accepted-decision-ref",
            "decision-log#db-security-check",
            "--context",
            "PostgreSQL changes need local evidence.",
            "--decision",
            "Run db_security_check before implementation.",
            "--consequence",
            "No remote database is required.",
            "--reversibility",
            "Remove the wrapper if Wrench absorbs the CLI contract.",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("# ADR Draft: Local DB checks"));
    assert!(stdout.contains("not automatically accepted"));
}

#[test]
fn deploy_dry_run_refuses_push_command() {
    let tmp = tempfile::tempdir().unwrap();

    let output = bolt_cosmatic()
        .args(["stack", "deploy_dry_run", "--root"])
        .arg(tmp.path())
        .args(["--cmd", "git push origin main", "--json"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("dry_run_only"));
    assert!(stdout.contains("refused"));
}
