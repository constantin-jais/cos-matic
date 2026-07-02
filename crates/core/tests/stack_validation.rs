use std::fs;

use bolt_cos_matic::stack::{self, StackDecision, StackSeverity};

#[test]
fn stack_detect_finds_rust_and_suggests_local_gates() {
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

    let report = stack::stack_detect(tmp.path()).unwrap();

    assert!(
        report
            .components
            .iter()
            .any(|component| component.name == "Rust")
    );
    assert!(
        report
            .suggested_commands
            .iter()
            .any(|command| command.contains("cargo test"))
    );
    assert!(
        report
            .missing_gates
            .iter()
            .any(|gate| gate.contains("cargo deny"))
    );
}

#[test]
fn dependency_audit_blocks_forbidden_provider_sdk() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(
        tmp.path().join("package.json"),
        r#"{
  "dependencies": {
    "@aws-sdk/client-s3": "latest"
  }
}"#,
    )
    .unwrap();

    let report = stack::dependency_audit(tmp.path()).unwrap();

    assert!(report.has_failures());
    assert!(report.findings.iter().any(|finding| {
        finding.severity == StackSeverity::Fail && finding.message.contains("AWS SDK")
    }));
}

#[test]
fn stack_scorecard_refuses_blocking_sovereignty_findings() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(
        tmp.path().join("Cargo.toml"),
        r#"
[package]
name = "demo"
version = "0.1.0"
edition = "2024"
[dependencies]
firebase = "0.0.1"
"#,
    )
    .unwrap();

    let report = stack::stack_scorecard(tmp.path()).unwrap();

    assert_eq!(report.decision, StackDecision::NoGo);
}

#[test]
fn stack_detect_ignores_deep_paths_beyond_scan_bound() {
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
    let mut deep = tmp.path().to_path_buf();
    for idx in 0..20 {
        deep = deep.join(format!("d{idx}"));
        fs::create_dir(&deep).unwrap();
    }
    fs::write(
        deep.join("package.json"),
        r#"{"scripts":{"build":"echo deep"}}"#,
    )
    .unwrap();

    let report = stack::stack_detect(tmp.path()).unwrap();

    assert!(
        report
            .components
            .iter()
            .any(|component| component.name == "Rust")
    );
    assert!(
        !report
            .components
            .iter()
            .any(|component| component.name == "Node/Bun web")
    );
}
