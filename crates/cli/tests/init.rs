//! Integration tests for `cosmatic init`.

use std::fs;
use std::process::Command;

const COSMATIC: &str = env!("CARGO_BIN_EXE_cosmatic");

#[test]
fn init_noninteractive_l0_creates_minimal_scaffold() {
    let tmp = tempfile::tempdir().unwrap();
    let status = Command::new(COSMATIC)
        .args([
            "init",
            "--name",
            "demo-project",
            "--level",
            "L0",
            "--adapter",
            "universal",
            "--yes",
        ])
        .current_dir(tmp.path())
        .status()
        .unwrap();

    assert!(status.success(), "cosmatic init --yes should succeed");

    // Assert files created.
    assert!(
        tmp.path().join("harness.toml").exists(),
        "harness.toml should be created"
    );
    assert!(
        tmp.path().join("domains/core-values.md").exists(),
        "domains/core-values.md should be created"
    );

    // Assert no workflow (L0 only).
    assert!(
        !tmp.path()
            .join(".github/workflows/orchestrator-loop.yml")
            .exists(),
        "L0 should not create workflow"
    );

    // Assert harness.toml contains project name and L0 autonomy.
    let manifest = fs::read_to_string(tmp.path().join("harness.toml")).unwrap();
    assert!(
        manifest.contains("name = \"demo-project\""),
        "harness.toml should contain project name"
    );
    assert!(
        manifest.contains("level = \"L0\""),
        "harness.toml should contain L0 autonomy level"
    );
    assert!(
        manifest.contains("universal"),
        "harness.toml should reference universal adapter"
    );
}

#[test]
fn init_noninteractive_l1_creates_workflow() {
    let tmp = tempfile::tempdir().unwrap();
    let status = Command::new(COSMATIC)
        .args([
            "init",
            "--name",
            "l1-project",
            "--level",
            "L1",
            "--adapter",
            "universal",
            "--yes",
        ])
        .current_dir(tmp.path())
        .status()
        .unwrap();

    assert!(status.success());
    assert!(
        tmp.path()
            .join(".github/workflows/orchestrator-loop.yml")
            .exists(),
        "L1 should create orchestrator workflow"
    );

    let workflow =
        fs::read_to_string(tmp.path().join(".github/workflows/orchestrator-loop.yml")).unwrap();
    assert!(
        workflow.contains("orchestrator-loop"),
        "workflow should have correct name"
    );
    assert!(
        workflow.contains("workflow_dispatch"),
        "workflow should be manually triggered"
    );
}

#[test]
fn init_noninteractive_l2_creates_workflow() {
    let tmp = tempfile::tempdir().unwrap();
    Command::new(COSMATIC)
        .args([
            "init",
            "--name",
            "l2-project",
            "--level",
            "L2",
            "--adapter",
            "universal",
            "--yes",
        ])
        .current_dir(tmp.path())
        .status()
        .unwrap();

    assert!(
        tmp.path()
            .join(".github/workflows/orchestrator-loop.yml")
            .exists(),
        "L2 should create workflow"
    );
}

#[test]
fn init_noninteractive_l3_creates_workflow() {
    let tmp = tempfile::tempdir().unwrap();
    Command::new(COSMATIC)
        .args([
            "init",
            "--name",
            "l3-project",
            "--level",
            "L3",
            "--adapter",
            "universal",
            "--yes",
        ])
        .current_dir(tmp.path())
        .status()
        .unwrap();

    assert!(
        tmp.path()
            .join(".github/workflows/orchestrator-loop.yml")
            .exists(),
        "L3 should create workflow"
    );
}

#[test]
fn init_is_idempotent_does_not_clobber_on_second_run() {
    let tmp = tempfile::tempdir().unwrap();

    // First run.
    let out1 = Command::new(COSMATIC)
        .args([
            "init",
            "--name",
            "test",
            "--level",
            "L0",
            "--adapter",
            "universal",
            "--yes",
        ])
        .current_dir(tmp.path())
        .output()
        .unwrap();

    assert!(out1.status.success(), "first init should succeed");

    let original_content = fs::read_to_string(tmp.path().join("harness.toml")).unwrap();

    // Second run—should warn about existing files, not overwrite.
    let out2 = Command::new(COSMATIC)
        .args([
            "init",
            "--name",
            "test",
            "--level",
            "L0",
            "--adapter",
            "universal",
            "--yes",
        ])
        .current_dir(tmp.path())
        .status()
        .unwrap();

    assert!(out2.success(), "second init should not error");

    let new_content = fs::read_to_string(tmp.path().join("harness.toml")).unwrap();
    assert_eq!(
        original_content, new_content,
        "harness.toml must not be modified on re-run"
    );
}

#[test]
fn init_errors_when_required_flags_missing_in_noninteractive_mode() {
    let tmp = tempfile::tempdir().unwrap();
    let status = Command::new(COSMATIC)
        .args(["init", "--yes"]) // No name, level, adapters.
        .current_dir(tmp.path())
        .status()
        .unwrap();

    assert!(!status.success(), "--yes without --name should fail");
}

#[test]
fn init_validates_autonomy_level() {
    let tmp = tempfile::tempdir().unwrap();
    let status = Command::new(COSMATIC)
        .args([
            "init",
            "--name",
            "test",
            "--level",
            "L99",
            "--adapter",
            "universal",
            "--yes",
        ])
        .current_dir(tmp.path())
        .status()
        .unwrap();

    assert!(!status.success(), "invalid level should fail");
}

#[test]
fn init_validates_adapter_names() {
    let tmp = tempfile::tempdir().unwrap();
    let status = Command::new(COSMATIC)
        .args([
            "init",
            "--name",
            "test",
            "--level",
            "L0",
            "--adapter",
            "unknown-adapter",
            "--yes",
        ])
        .current_dir(tmp.path())
        .status()
        .unwrap();

    assert!(!status.success(), "invalid adapter should fail");
}

#[test]
fn init_defaults_level_and_adapter_in_noninteractive_mode() {
    let tmp = tempfile::tempdir().unwrap();
    let status = Command::new(COSMATIC)
        .args(["init", "--name", "defaults-test", "--yes"])
        .current_dir(tmp.path())
        .status()
        .unwrap();

    assert!(
        status.success(),
        "should succeed with name only (use defaults)"
    );

    let manifest = fs::read_to_string(tmp.path().join("harness.toml")).unwrap();
    assert!(manifest.contains("level = \"L0\""), "should default to L0");
    assert!(
        manifest.contains("universal"),
        "should default to universal adapter"
    );
}

#[test]
fn init_multiple_adapters_creates_all_targets() {
    let tmp = tempfile::tempdir().unwrap();
    let status = Command::new(COSMATIC)
        .args([
            "init",
            "--name",
            "multi",
            "--level",
            "L0",
            "--adapter",
            "universal",
            "--adapter",
            "claude",
            "--adapter",
            "cursor",
            "--yes",
        ])
        .current_dir(tmp.path())
        .status()
        .unwrap();

    assert!(status.success());

    let manifest = fs::read_to_string(tmp.path().join("harness.toml")).unwrap();
    assert!(
        manifest.contains(r#"adapter = "universal""#),
        "should have universal target"
    );
    assert!(
        manifest.contains(r#"adapter = "claude""#),
        "should have claude target"
    );
    assert!(
        manifest.contains(r#"adapter = "cursor""#),
        "should have cursor target"
    );
}

#[test]
fn init_creates_valid_harness_toml_that_aom_generate_accepts() {
    let tmp = tempfile::tempdir().unwrap();
    let status = Command::new(COSMATIC)
        .args([
            "init",
            "--name",
            "valid-test",
            "--level",
            "L0",
            "--adapter",
            "universal",
            "--yes",
        ])
        .current_dir(tmp.path())
        .status()
        .unwrap();

    assert!(status.success());

    // Verify `cosmatic generate` accepts and compiles the generated manifest.
    let gen_status = Command::new(COSMATIC)
        .args(["generate"])
        .current_dir(tmp.path())
        .status()
        .unwrap();

    assert!(
        gen_status.success(),
        "cosmatic generate should succeed on init scaffold"
    );

    // Verify outputs are now up to date with `--check`.
    let check_status = Command::new(COSMATIC)
        .args(["generate", "--check"])
        .current_dir(tmp.path())
        .status()
        .unwrap();

    assert!(
        check_status.success(),
        "generated manifest should be in sync after `cosmatic generate`"
    );
}

#[test]
fn init_prints_checklist_for_l1() {
    let tmp = tempfile::tempdir().unwrap();
    let output = Command::new(COSMATIC)
        .args([
            "init",
            "--name",
            "l1-test",
            "--level",
            "L1",
            "--adapter",
            "universal",
            "--yes",
        ])
        .current_dir(tmp.path())
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Operator Checklist"),
        "should print operator checklist"
    );
    assert!(
        stdout.contains("L1"),
        "should mention L1 level in checklist"
    );
}

#[test]
fn init_l2_checklist_matches_workflow_credentials() {
    let tmp = tempfile::tempdir().unwrap();
    let output = Command::new(COSMATIC)
        .args([
            "init",
            "--name",
            "l2-test",
            "--level",
            "L2",
            "--adapter",
            "universal",
            "--yes",
        ])
        .current_dir(tmp.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("cosmatic_BOT_TOKEN"),
        "checklist should ask for the workflow's write-token secret"
    );
    assert!(
        stdout.contains("cosmatic_CHECKS_TOKEN is supplied by the workflow"),
        "checklist should explain the checks token is not a user-created secret"
    );
    assert!(
        !stdout.contains("GITHUB_TOKEN (for git push"),
        "checklist must not ask users to create the obsolete GITHUB_TOKEN secret"
    );
    assert!(
        stdout.contains("require human approval before merge"),
        "L2 should remain the approve-before-merge mode"
    );
}

#[test]
fn init_l3_checklist_allows_green_gate_bot_merge() {
    let tmp = tempfile::tempdir().unwrap();
    let output = Command::new(COSMATIC)
        .args([
            "init",
            "--name",
            "l3-test",
            "--level",
            "L3",
            "--adapter",
            "universal",
            "--yes",
        ])
        .current_dir(tmp.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Do not require human approval for the bot merge path"),
        "L3 should not instruct users to block the bot on mandatory review"
    );
    assert!(
        stdout.contains("Confirm the bot can merge after the gate is green"),
        "L3 should explicitly preserve the green-gate requirement"
    );
}

#[test]
fn init_with_repo_flag_includes_it() {
    let tmp = tempfile::tempdir().unwrap();
    let status = Command::new(COSMATIC)
        .args([
            "init",
            "--name",
            "repo-test",
            "--level",
            "L0",
            "--adapter",
            "universal",
            "--repo",
            "owner/repo",
            "--yes",
        ])
        .current_dir(tmp.path())
        .status()
        .unwrap();

    assert!(status.success());
    // Note: repo is parsed but not yet reflected in harness.toml (future enhancement).
}

#[test]
fn init_project_name_validation_rejects_invalid_chars() {
    let tmp = tempfile::tempdir().unwrap();
    let status = Command::new(COSMATIC)
        .args([
            "init",
            "--name",
            "invalid@project!",
            "--level",
            "L0",
            "--adapter",
            "universal",
            "--yes",
        ])
        .current_dir(tmp.path())
        .status()
        .unwrap();

    assert!(
        !status.success(),
        "project name with invalid chars should fail"
    );
}

#[test]
fn init_core_values_template_contains_project_name() {
    let tmp = tempfile::tempdir().unwrap();
    Command::new(COSMATIC)
        .args([
            "init",
            "--name",
            "myproject",
            "--level",
            "L0",
            "--adapter",
            "universal",
            "--yes",
        ])
        .current_dir(tmp.path())
        .status()
        .unwrap();

    let core_values = fs::read_to_string(tmp.path().join("domains/core-values.md")).unwrap();
    assert!(
        core_values.contains("myproject"),
        "core-values.md should include project name"
    );
}
