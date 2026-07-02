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

#[test]
fn db_security_check_accepts_fixture_sql_without_pii() {
    let tmp = tempfile::tempdir().unwrap();
    let fixtures = tmp.path().join("fixtures");
    fs::create_dir(&fixtures).unwrap();
    fs::write(
        fixtures.join("tenant_embeddings.sql"),
        r#"
create table embeddings (
  tenant_id uuid not null,
  embedding vector(3) not null
);
alter table embeddings enable row level security;
create policy tenant_embeddings on embeddings using (tenant_id = current_setting('app.tenant_id')::uuid);
insert into embeddings (tenant_id, embedding) values ('00000000-0000-0000-0000-000000000001', '[1,2,3]');
"#,
    )
    .unwrap();

    let report = stack::db_security_check(
        tmp.path(),
        &stack::DbSecurityCheckOptions {
            database_url_requested: false,
            allow_db_connection: false,
        },
    )
    .unwrap();

    assert!(!report.has_failures());
    assert_eq!(report.mode, "local_only");
    assert!(
        report
            .accepted_fixtures
            .iter()
            .any(|path| path.ends_with("tenant_embeddings.sql"))
    );
}

#[test]
fn db_security_check_refuses_row_security_off() {
    let tmp = tempfile::tempdir().unwrap();
    fs::create_dir(tmp.path().join("migrations")).unwrap();
    fs::write(
        tmp.path().join("migrations/0001_bad.sql"),
        "set row_security = off;",
    )
    .unwrap();

    let report = stack::db_security_check(
        tmp.path(),
        &stack::DbSecurityCheckOptions {
            database_url_requested: false,
            allow_db_connection: false,
        },
    )
    .unwrap();

    assert!(report.has_failures());
    assert!(report.findings.iter().any(|finding| {
        finding.severity == StackSeverity::Fail && finding.message.contains("row_security = off")
    }));
}

#[test]
fn db_security_check_detects_multitenant_sql_without_rls() {
    let tmp = tempfile::tempdir().unwrap();
    fs::create_dir(tmp.path().join("migrations")).unwrap();
    fs::write(
        tmp.path().join("migrations/0001_tenant.sql"),
        "create table documents (tenant_id uuid not null, body text not null);",
    )
    .unwrap();

    let report = stack::db_security_check(
        tmp.path(),
        &stack::DbSecurityCheckOptions {
            database_url_requested: false,
            allow_db_connection: false,
        },
    )
    .unwrap();

    assert!(report.has_failures());
    assert!(report.findings.iter().any(|finding| {
        finding.severity == StackSeverity::Fail && finding.message.contains("without RLS")
    }));
}

#[test]
fn adr_generate_reports_missing_fields_and_never_accepts() {
    let report = stack::adr_generate(&stack::AdrDraftRequest {
        title: Some("Use local-only DB checks".to_string()),
        accepted_decision_ref: None,
        context: None,
        decision: Some("Run static SQL checks before DB work".to_string()),
        consequences: Vec::new(),
        reversibility: None,
    });

    assert!(!report.is_complete());
    assert!(!report.accepted_automatically);
    assert!(report.missing_fields.contains(&"context".to_string()));
    assert!(report.markdown.contains("Status: Draft"));
}

#[test]
fn deploy_dry_run_refuses_deploy_push_provision_apply() {
    let tmp = tempfile::tempdir().unwrap();

    let report = stack::deploy_dry_run(
        tmp.path(),
        &[
            "cargo test".to_string(),
            "git push origin main".to_string(),
            "terraform apply".to_string(),
        ],
    )
    .unwrap();

    assert!(report.dry_run_only);
    assert!(report.has_failures());
    assert_eq!(
        report
            .commands
            .iter()
            .filter(|command| command.status == "refused")
            .count(),
        2
    );
}
