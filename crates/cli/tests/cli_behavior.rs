//! Black-box behavioral tests on the `bolt-cosmatic` binary itself.
//!
//! The library-level e2e tests in `crates/core` exercise `generate::run`, but the
//! binary's *observable output* — the per-file report, the graceful-degradation
//! warnings on stderr, and the up-to-date message — went untested. The workspace
//! split relocated `run()` into this crate and nearly dropped the warnings loop
//! and reverted the "file(s)" wording; nothing was red. These tests pin that
//! surface so such a regression fails loudly.

use std::fs;
use std::process::Command;

use ed25519_dalek::{Signer, SigningKey};

/// Path to the compiled `bolt-cosmatic` binary, injected by Cargo for integration tests of
/// the crate that defines the bin.
const COSMATIC: &str = env!("CARGO_BIN_EXE_bolt-cosmatic");

fn repo_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

#[test]
fn cli_help_uses_bolt_cosmatic_name() {
    let out = Command::new(COSMATIC)
        .arg("--help")
        .output()
        .expect("spawn bolt-cosmatic --help");

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "stdout:\n{stdout}\nstderr:\n{stderr}");
    assert!(
        stdout.contains("Usage: bolt-cosmatic <COMMAND>"),
        "help should advertise the installed command name; stdout:\n{stdout}"
    );
}

#[test]
fn live_loop_smoke_uses_bolt_cosmatic_binary() {
    let script = fs::read_to_string(repo_root().join("scripts/live-loop-smoke.sh"))
        .expect("read live-loop-smoke.sh");

    assert!(
        script.contains("--bin bolt-cosmatic"),
        "live smoke script must run the real binary target; script:\n{script}"
    );
    assert!(
        !script.contains("--bin aom"),
        "live smoke script must not reference the stale aom binary target"
    );
}

#[test]
fn root_harness_is_present_and_in_sync() {
    let root = repo_root();
    let manifest = root.join("harness.toml");
    assert!(manifest.exists(), "root harness.toml should exist");

    let out = Command::new(COSMATIC)
        .args(["generate", "--check", "--manifest"])
        .arg(&manifest)
        .current_dir(&root)
        .output()
        .expect("spawn bolt-cosmatic generate --check --manifest harness.toml");

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "root harness should be in sync\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

#[test]
fn check_prints_files_up_to_date_message() {
    // The committed example is in sync, so `--check` must exit 0 and print the
    // file-count message. Pins the "N file(s) up to date" wording — a refactor
    // once reverted it to "target(s)".
    let example = repo_root().join("examples/minimal");
    let out = Command::new(COSMATIC)
        .args(["generate", "--check"])
        .current_dir(&example)
        .output()
        .expect("spawn bolt-cosmatic generate --check");

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

    let out = Command::new(COSMATIC)
        .arg("generate")
        .current_dir(root)
        .output()
        .expect("spawn bolt-cosmatic generate");

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

#[test]
fn maturity_validate_json_reports_contract_first_claim() {
    let tmp = tempfile::tempdir().unwrap();
    let claim = tmp.path().join("maturity.json");
    fs::write(&claim, maturity_claim("R1", "R3", r#""core":{"level":"R1","status":"warn","evidence":["crates/core/src/p0_contract.rs"]}"#)).unwrap();

    let out = Command::new(COSMATIC)
        .args(["maturity", "validate", claim.to_str().unwrap(), "--json"])
        .output()
        .expect("spawn bolt-cosmatic maturity validate --json");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "stdout:\n{stdout}\nstderr:\n{stderr}");
    assert!(
        stdout.contains("\"project\": \"rumble-lm\""),
        "stdout:\n{stdout}"
    );
    assert!(
        stdout.contains("\"current_level\": \"R1\""),
        "stdout:\n{stdout}"
    );
}

#[test]
fn maturity_validate_rejects_mobile_without_portable_core() {
    let tmp = tempfile::tempdir().unwrap();
    let claim = tmp.path().join("maturity.json");
    fs::write(
        &claim,
        maturity_claim(
            "R7",
            "R10",
            r#""core":{"level":"R0","status":"blocked","evidence":[]}"#,
        ),
    )
    .unwrap();

    let out = Command::new(COSMATIC)
        .args(["maturity", "validate", claim.to_str().unwrap(), "--json"])
        .output()
        .expect("spawn bolt-cosmatic maturity validate --json");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(!out.status.success(), "stdout:\n{stdout}");
    assert!(
        stdout.contains("mobile_without_portable_core"),
        "stdout:\n{stdout}"
    );
}

#[test]
fn maturity_report_summarizes_directory() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("lm.json"), maturity_claim("R1", "R3", r#""core":{"level":"R1","status":"warn","evidence":["crates/core/src/p0_contract.rs"]}"#)).unwrap();

    let out = Command::new(COSMATIC)
        .args(["maturity", "report", tmp.path().to_str().unwrap()])
        .output()
        .expect("spawn bolt-cosmatic maturity report");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "stdout:\n{stdout}\nstderr:\n{stderr}");
    assert!(stdout.contains("rumble-lm"), "stdout:\n{stdout}");
    assert!(stdout.contains("current=R1"), "stdout:\n{stdout}");
}

fn maturity_claim(current: &str, target: &str, core_axis: &str) -> String {
    format!(
        r#"{{
          "format":"rumble.delivery_maturity.v0.1",
          "project":{{"name":"rumble-lm","layer":"Rumble","role":"Pedagogy and grounding dojo","maturity_mode":"contract-first"}},
          "claimed_at":"2026-06-30T00:00:00Z",
          "current_level":"{current}","target_level":"{target}","next_level":"R2",
          "promotion_candidate":{{"from":"R1","to":"R2","status":"blocked","blocked_by":["portable-core gate missing"]}},
          "axes":{{
            "spec":{{"level":"R1","status":"pass","evidence":["README.md"]}},
            "contracts":{{"level":"R1","status":"pass","evidence":["contract.md"]}},
            {core_axis},
            "security":{{"level":"R1","status":"warn","evidence":["auth.md"]}},
            "ux":{{"level":"R0","status":"blocked","evidence":[]}},
            "persistence":{{"level":"R0","status":"blocked","evidence":[]}},
            "orchestration":{{"level":"R1","status":"warn","evidence":["handoff.md"]}},
            "inspection":{{"level":"R0","status":"warn","evidence":[]}},
            "release":{{"level":"R0","status":"blocked","evidence":[]}},
            "operations":{{"level":"R0","status":"blocked","evidence":[]}},
            "commercial_readiness":{{"level":"R0","status":"blocked","evidence":[]}},
            "learning_yield":{{"level":"R1","status":"pass","evidence":["decision-log.md"]}}
          }},
          "platform_readiness":{{"cli":"none","api":"proof","web":"none","desktop":"none","mobile":"none","self_hosted":"planned","cloud_eu":"planned"}},
          "evidence":[{{"kind":"doc","ref":"README.md","status":"present"}}],
          "learning_yield":[{{"kind":"evidence_produced","description":"The claim proves maturity reporting.","owner":"harness"}}]
        }}"#
    )
}

#[test]
fn handoff_validate_json_reports_valid_payload() {
    let tmp = tempfile::tempdir().unwrap();
    let payload = tmp.path().join("handoff.json");
    fs::write(
        &payload,
        r#"{
          "format":"canvas.bolt_handoff.v0.1",
          "kind":"planning_request",
          "source":{"product":"rumble-canvas","workspace_id":"w","handoff_id":"h","created_by":"a","created_at":"2026-06-30T00:00:00Z"},
          "package":{"package_id":"p","version":"0.1.0","package_hash":"sha256:0000000000000000000000000000000000000000000000000000000000000000","items":[{"section_id":"s","revision_id":"r"}]},
          "planning_scope":{"mode":"full_package","target_objects":[],"excluded_objects":[],"goal":"Plan only"},
          "spec_context":{},
          "traceability_links":[{"source_type":"journey","source_id":"j","target_type":"action","target_id":"a","relation_type":"implements"}],
          "active_waivers":[],"open_questions":[],"risks":[],"capability_candidates":[],
          "constraints":{"sovereignty":"self-hostable","data_residency":"EU","non_goals":[]},
          "requested_outputs":["implementation_plan"],
          "execution_policy":{"planning_only":true,"allow_execution":false,"requires_human_approval_for_execution":true}
        }"#,
    )
    .unwrap();

    let out = Command::new(COSMATIC)
        .args(["handoff", "validate", payload.to_str().unwrap(), "--json"])
        .output()
        .expect("spawn bolt-cosmatic handoff validate --json");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "stdout:\n{stdout}\nstderr:\n{stderr}");
    assert!(
        stdout.contains("\"handoff_id\": \"h\""),
        "stdout:\n{stdout}"
    );
}

#[test]
fn handoff_validate_uses_canonical_execution_policy_refusal_code() {
    let tmp = tempfile::tempdir().unwrap();
    let payload = tmp.path().join("handoff.json");
    fs::write(&payload, handoff_payload_with(r#""execution_policy":{"planning_only":false,"allow_execution":true,"requires_human_approval_for_execution":false}"#)).unwrap();

    let out = Command::new(COSMATIC)
        .args(["handoff", "validate", payload.to_str().unwrap(), "--json"])
        .output()
        .expect("spawn bolt-cosmatic handoff validate --json");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(!out.status.success(), "stdout:\n{stdout}");
    assert!(
        stdout.contains("execution_policy_forbidden"),
        "stdout:\n{stdout}"
    );
}

#[test]
fn handoff_validate_rejects_artifact_ref_without_hash() {
    let tmp = tempfile::tempdir().unwrap();
    let payload = tmp.path().join("handoff.json");
    fs::write(&payload, handoff_payload_with(r#""package":{"package_id":"p","version":"0.1.0","package_hash":"sha256:0000000000000000000000000000000000000000000000000000000000000000","artifact_reference_id":"artifact:p","items":[{"section_id":"s","revision_id":"r"}]}"#)).unwrap();

    let out = Command::new(COSMATIC)
        .args(["handoff", "validate", payload.to_str().unwrap(), "--json"])
        .output()
        .expect("spawn bolt-cosmatic handoff validate --json");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(!out.status.success(), "stdout:\n{stdout}");
    assert!(
        stdout.contains("artifact_integrity_failed"),
        "stdout:\n{stdout}"
    );
}

#[test]
fn handoff_validate_rejects_sovereignty_violation() {
    let tmp = tempfile::tempdir().unwrap();
    let payload = tmp.path().join("handoff.json");
    fs::write(&payload, handoff_payload_with(r#""constraints":{"sovereignty":"violated: mandatory US SaaS for core truth","data_residency":"unknown","non_goals":[],"requires_external_saas":true}"#)).unwrap();

    let out = Command::new(COSMATIC)
        .args(["handoff", "validate", payload.to_str().unwrap(), "--json"])
        .output()
        .expect("spawn bolt-cosmatic handoff validate --json");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(!out.status.success(), "stdout:\n{stdout}");
    assert!(
        stdout.contains("sovereignty_policy_violation"),
        "stdout:\n{stdout}"
    );
}

#[test]
fn handoff_validate_rejects_handoff_hash_conflict() {
    let tmp = tempfile::tempdir().unwrap();
    let payload = tmp.path().join("handoff.json");
    fs::write(&payload, handoff_payload_with(r#""idempotency":{"prior_payload_hash":"sha256:1111111111111111111111111111111111111111111111111111111111111111","payload_hash":"sha256:2222222222222222222222222222222222222222222222222222222222222222"}"#)).unwrap();

    let out = Command::new(COSMATIC)
        .args(["handoff", "validate", payload.to_str().unwrap(), "--json"])
        .output()
        .expect("spawn bolt-cosmatic handoff validate --json");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(!out.status.success(), "stdout:\n{stdout}");
    assert!(
        stdout.contains("handoff_hash_conflict"),
        "stdout:\n{stdout}"
    );
}

fn handoff_payload_with(replacement_field: &str) -> String {
    let mut fields = vec![
        r#""format":"canvas.bolt_handoff.v0.1""#,
        r#""kind":"planning_request""#,
        r#""source":{"product":"rumble-canvas","workspace_id":"w","handoff_id":"h","created_by":"a","created_at":"2026-06-30T00:00:00Z"}"#,
        r#""package":{"package_id":"p","version":"0.1.0","package_hash":"sha256:0000000000000000000000000000000000000000000000000000000000000000","artifact_reference_id":null,"items":[{"section_id":"s","revision_id":"r"}]}"#,
        r#""planning_scope":{"mode":"full_package","target_objects":[],"excluded_objects":[],"goal":"Plan only"}"#,
        r#""spec_context":{}"#,
        r#""traceability_links":[{"source_type":"journey","source_id":"j","target_type":"action","target_id":"a","relation_type":"implements"}]"#,
        r#""active_waivers":[]"#,
        r#""open_questions":[]"#,
        r#""risks":[]"#,
        r#""capability_candidates":[]"#,
        r#""constraints":{"sovereignty":"self-hostable","data_residency":"EU","non_goals":[]}"#,
        r#""requested_outputs":["implementation_plan"]"#,
        r#""execution_policy":{"planning_only":true,"allow_execution":false,"requires_human_approval_for_execution":true}"#,
    ];
    let key = replacement_field.split(':').next().unwrap_or_default();
    fields.retain(|field| !field.starts_with(key));
    fields.push(replacement_field);
    format!("{{{}}}", fields.join(","))
}

#[test]
fn handoff_plan_requires_dry_run() {
    let tmp = tempfile::tempdir().unwrap();
    let payload = tmp.path().join("handoff.json");
    fs::write(&payload, "{}").unwrap();
    let out = Command::new(COSMATIC)
        .args(["handoff", "plan", payload.to_str().unwrap()])
        .output()
        .expect("spawn bolt-cosmatic handoff plan");
    assert!(!out.status.success(), "plan without --dry-run must fail");
}

#[test]
fn handoff_plan_projects_passed_wrench_evidence_report() {
    let tmp = tempfile::tempdir().unwrap();
    let payload = tmp.path().join("handoff.json");
    let evidence = tmp.path().join("wrench-evidence.json");
    fs::write(
        &payload,
        handoff_payload_with(r#""requested_outputs":["implementation_plan"]"#),
    )
    .unwrap();
    fs::write(&evidence, wrench_evidence_report("passed")).unwrap();

    let out = Command::new(COSMATIC)
        .args([
            "handoff",
            "plan",
            payload.to_str().unwrap(),
            "--dry-run",
            "--json",
            "--evidence-report",
            evidence.to_str().unwrap(),
        ])
        .output()
        .expect("spawn bolt-cosmatic handoff plan --evidence-report");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "stdout:\n{stdout}\nstderr:\n{stderr}");
    assert!(stdout.contains("wrench_report_passed"), "stdout:\n{stdout}");
    assert!(
        stdout.contains("Wrench evidence refs are present"),
        "stdout:\n{stdout}"
    );
}

#[test]
fn handoff_plan_projects_gear_wrench_evidence_manifest() {
    let tmp = tempfile::tempdir().unwrap();
    let payload = tmp.path().join("handoff.json");
    let manifest = tmp.path().join("gear-wrench-manifest.json");
    fs::write(
        &payload,
        handoff_payload_with(r#""requested_outputs":["implementation_plan"]"#),
    )
    .unwrap();
    fs::write(
        &manifest,
        gear_wrench_evidence_manifest(
            "passed",
            "sha256:5555555555555555555555555555555555555555555555555555555555555555",
            "active",
        ),
    )
    .unwrap();

    let out = Command::new(COSMATIC)
        .args([
            "handoff",
            "plan",
            payload.to_str().unwrap(),
            "--dry-run",
            "--json",
            "--evidence-manifest",
            manifest.to_str().unwrap(),
        ])
        .output()
        .expect("spawn bolt-cosmatic handoff plan --evidence-manifest");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "stdout:\n{stdout}\nstderr:\n{stderr}");

    let plan: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let gates = plan["gates"].as_array().expect("gates array");
    let wrench_gate = gates
        .iter()
        .find(|gate| gate["code"] == "wrench_report_passed")
        .expect("wrench_report_passed gate");
    let artifact_gate = gates
        .iter()
        .find(|gate| gate["code"] == "artifact_supply_chain_verified")
        .expect("artifact_supply_chain_verified gate");
    assert_eq!(wrench_gate["status"], "pass");
    assert_eq!(artifact_gate["status"], "pass");
    assert!(
        artifact_gate["detail"]
            .as_str()
            .unwrap()
            .contains("Gear artifact refs are present"),
        "artifact gate: {artifact_gate}"
    );
}

#[test]
fn handoff_plan_refuses_invalid_gear_wrench_evidence_manifest_hash() {
    let tmp = tempfile::tempdir().unwrap();
    let payload = tmp.path().join("handoff.json");
    let manifest = tmp.path().join("gear-wrench-manifest.json");
    fs::write(
        &payload,
        handoff_payload_with(r#""requested_outputs":["implementation_plan"]"#),
    )
    .unwrap();
    fs::write(
        &manifest,
        gear_wrench_evidence_manifest("passed", "sha256:not-a-valid-hash", "active"),
    )
    .unwrap();

    let out = Command::new(COSMATIC)
        .args([
            "handoff",
            "plan",
            payload.to_str().unwrap(),
            "--dry-run",
            "--json",
            "--evidence-manifest",
            manifest.to_str().unwrap(),
        ])
        .output()
        .expect("spawn bolt-cosmatic handoff plan --evidence-manifest");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "invalid manifest hash must refuse");
    assert!(
        stderr.contains("artifact.hash must be sha256"),
        "stderr:\n{stderr}"
    );
}

#[test]
fn handoff_plan_refuses_revoked_gear_wrench_evidence_manifest() {
    let tmp = tempfile::tempdir().unwrap();
    let payload = tmp.path().join("handoff.json");
    let manifest = tmp.path().join("gear-wrench-manifest.json");
    fs::write(
        &payload,
        handoff_payload_with(r#""requested_outputs":["implementation_plan"]"#),
    )
    .unwrap();
    fs::write(
        &manifest,
        gear_wrench_evidence_manifest(
            "passed",
            "sha256:5555555555555555555555555555555555555555555555555555555555555555",
            "revoked",
        ),
    )
    .unwrap();

    let out = Command::new(COSMATIC)
        .args([
            "handoff",
            "plan",
            payload.to_str().unwrap(),
            "--dry-run",
            "--json",
            "--evidence-manifest",
            manifest.to_str().unwrap(),
        ])
        .output()
        .expect("spawn bolt-cosmatic handoff plan --evidence-manifest");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(!out.status.success(), "revoked manifest must refuse");
    assert!(stdout.contains("artifact_not_active"), "stdout:\n{stdout}");
    assert!(
        stdout.contains("artifact_supply_chain_verified"),
        "stdout:\n{stdout}"
    );
}

#[test]
fn handoff_plan_projects_signed_human_approval() {
    let tmp = tempfile::tempdir().unwrap();
    let payload = tmp.path().join("handoff.json");
    let approval = tmp.path().join("human-approval.json");
    let registry = tmp.path().join("approval-key-registry.json");
    fs::write(
        &payload,
        handoff_payload_with(r#""requested_outputs":["implementation_plan"]"#),
    )
    .unwrap();
    fs::write(
        &approval,
        human_approval(
            "approved",
            "sha256:0000000000000000000000000000000000000000000000000000000000000000",
            "2099-12-31T00:00:00Z",
        ),
    )
    .unwrap();
    fs::write(&registry, approval_key_registry(&[default_approval_key()])).unwrap();

    let out = Command::new(COSMATIC)
        .args([
            "handoff",
            "plan",
            payload.to_str().unwrap(),
            "--dry-run",
            "--json",
            "--human-approval",
            approval.to_str().unwrap(),
            "--approval-key-registry",
            registry.to_str().unwrap(),
        ])
        .output()
        .expect("spawn bolt-cosmatic handoff plan --human-approval");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "stdout:\n{stdout}\nstderr:\n{stderr}");

    let plan: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let approval_gate = plan["gates"]
        .as_array()
        .expect("gates array")
        .iter()
        .find(|gate| gate["code"] == "human_approval_checkpoint")
        .expect("human_approval_checkpoint gate");
    assert_eq!(approval_gate["status"], "pass");
    assert!(
        approval_gate["detail"]
            .as_str()
            .unwrap()
            .contains("registry-backed"),
        "approval gate: {approval_gate}"
    );
}

#[test]
fn handoff_plan_accepts_rotated_human_approval_key() {
    let tmp = tempfile::tempdir().unwrap();
    let payload = tmp.path().join("handoff.json");
    let approval = tmp.path().join("human-approval.json");
    let registry = tmp.path().join("approval-key-registry.json");
    fs::write(
        &payload,
        handoff_payload_with(r#""requested_outputs":["implementation_plan"]"#),
    )
    .unwrap();
    fs::write(
        &approval,
        human_approval_with_key(
            "approved",
            "sha256:0000000000000000000000000000000000000000000000000000000000000000",
            "2099-12-31T00:00:00Z",
            "human-operator-demo-key-02",
            43,
        ),
    )
    .unwrap();
    fs::write(
        &registry,
        approval_key_registry(&[
            ApprovalKeyFixture {
                public_key_ref: "human-operator-demo-key-01",
                seed: 42,
                state: "active",
                not_before: "2000-01-01T00:00:00Z",
                expires_at: "2099-12-31T00:00:00Z",
                rotated_from: None,
                rotated_to: Some("human-operator-demo-key-02"),
                revoked_at: None,
            },
            ApprovalKeyFixture {
                public_key_ref: "human-operator-demo-key-02",
                seed: 43,
                state: "active",
                not_before: "2000-07-01T00:00:00Z",
                expires_at: "2099-12-31T00:00:00Z",
                rotated_from: Some("human-operator-demo-key-01"),
                rotated_to: None,
                revoked_at: None,
            },
        ]),
    )
    .unwrap();

    let out = Command::new(COSMATIC)
        .args([
            "handoff",
            "plan",
            payload.to_str().unwrap(),
            "--dry-run",
            "--json",
            "--human-approval",
            approval.to_str().unwrap(),
            "--approval-key-registry",
            registry.to_str().unwrap(),
        ])
        .output()
        .expect("spawn bolt-cosmatic handoff plan with rotated approval key");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "stdout:\n{stdout}\nstderr:\n{stderr}");
    assert!(
        stdout.contains("human_approval_checkpoint"),
        "stdout:\n{stdout}"
    );
}

#[test]
fn handoff_plan_refuses_unknown_human_approval_key() {
    let tmp = tempfile::tempdir().unwrap();
    let payload = tmp.path().join("handoff.json");
    let approval = tmp.path().join("human-approval.json");
    fs::write(
        &payload,
        handoff_payload_with(r#""requested_outputs":["implementation_plan"]"#),
    )
    .unwrap();
    fs::write(
        &approval,
        human_approval(
            "approved",
            "sha256:0000000000000000000000000000000000000000000000000000000000000000",
            "2099-12-31T00:00:00Z",
        ),
    )
    .unwrap();

    let out = Command::new(COSMATIC)
        .args([
            "handoff",
            "plan",
            payload.to_str().unwrap(),
            "--dry-run",
            "--json",
            "--human-approval",
            approval.to_str().unwrap(),
        ])
        .output()
        .expect("spawn bolt-cosmatic handoff plan --human-approval");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(!out.status.success(), "unknown approval key must refuse");
    assert!(
        stdout.contains("human_approval_key_unknown"),
        "stdout:\n{stdout}"
    );
}

#[test]
fn handoff_plan_refuses_revoked_human_approval_key() {
    let tmp = tempfile::tempdir().unwrap();
    let payload = tmp.path().join("handoff.json");
    let approval = tmp.path().join("human-approval.json");
    let registry = tmp.path().join("approval-key-registry.json");
    fs::write(
        &payload,
        handoff_payload_with(r#""requested_outputs":["implementation_plan"]"#),
    )
    .unwrap();
    fs::write(
        &approval,
        human_approval(
            "approved",
            "sha256:0000000000000000000000000000000000000000000000000000000000000000",
            "2099-12-31T00:00:00Z",
        ),
    )
    .unwrap();
    fs::write(
        &registry,
        approval_key_registry(&[ApprovalKeyFixture {
            state: "revoked",
            revoked_at: Some("2026-07-02T00:00:00Z"),
            ..default_approval_key()
        }]),
    )
    .unwrap();

    let out = Command::new(COSMATIC)
        .args([
            "handoff",
            "plan",
            payload.to_str().unwrap(),
            "--dry-run",
            "--json",
            "--human-approval",
            approval.to_str().unwrap(),
            "--approval-key-registry",
            registry.to_str().unwrap(),
        ])
        .output()
        .expect("spawn bolt-cosmatic handoff plan --human-approval");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(!out.status.success(), "revoked approval key must refuse");
    assert!(
        stdout.contains("human_approval_key_not_active"),
        "stdout:\n{stdout}"
    );
}

#[test]
fn handoff_plan_refuses_expired_human_approval_key() {
    let tmp = tempfile::tempdir().unwrap();
    let payload = tmp.path().join("handoff.json");
    let approval = tmp.path().join("human-approval.json");
    let registry = tmp.path().join("approval-key-registry.json");
    fs::write(
        &payload,
        handoff_payload_with(r#""requested_outputs":["implementation_plan"]"#),
    )
    .unwrap();
    fs::write(
        &approval,
        human_approval(
            "approved",
            "sha256:0000000000000000000000000000000000000000000000000000000000000000",
            "2099-12-31T00:00:00Z",
        ),
    )
    .unwrap();
    fs::write(
        &registry,
        approval_key_registry(&[ApprovalKeyFixture {
            expires_at: "2000-01-01T00:00:00Z",
            ..default_approval_key()
        }]),
    )
    .unwrap();

    let out = Command::new(COSMATIC)
        .args([
            "handoff",
            "plan",
            payload.to_str().unwrap(),
            "--dry-run",
            "--json",
            "--human-approval",
            approval.to_str().unwrap(),
            "--approval-key-registry",
            registry.to_str().unwrap(),
        ])
        .output()
        .expect("spawn bolt-cosmatic handoff plan --human-approval");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(!out.status.success(), "expired approval key must refuse");
    assert!(
        stdout.contains("human_approval_key_not_active"),
        "stdout:\n{stdout}"
    );
}

#[test]
fn handoff_plan_refuses_rejected_human_approval() {
    let tmp = tempfile::tempdir().unwrap();
    let payload = tmp.path().join("handoff.json");
    let approval = tmp.path().join("human-approval.json");
    let registry = tmp.path().join("approval-key-registry.json");
    fs::write(
        &payload,
        handoff_payload_with(r#""requested_outputs":["implementation_plan"]"#),
    )
    .unwrap();
    fs::write(
        &approval,
        human_approval(
            "rejected",
            "sha256:0000000000000000000000000000000000000000000000000000000000000000",
            "2099-12-31T00:00:00Z",
        ),
    )
    .unwrap();
    fs::write(&registry, approval_key_registry(&[default_approval_key()])).unwrap();

    let out = Command::new(COSMATIC)
        .args([
            "handoff",
            "plan",
            payload.to_str().unwrap(),
            "--dry-run",
            "--json",
            "--human-approval",
            approval.to_str().unwrap(),
            "--approval-key-registry",
            registry.to_str().unwrap(),
        ])
        .output()
        .expect("spawn bolt-cosmatic handoff plan --human-approval");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(!out.status.success(), "rejected approval must refuse");
    assert!(
        stdout.contains("human_approval_not_approved"),
        "stdout:\n{stdout}"
    );
    assert!(
        stdout.contains("human_approval_checkpoint"),
        "stdout:\n{stdout}"
    );
}

#[test]
fn handoff_plan_refuses_expired_human_approval() {
    let tmp = tempfile::tempdir().unwrap();
    let payload = tmp.path().join("handoff.json");
    let approval = tmp.path().join("human-approval.json");
    let registry = tmp.path().join("approval-key-registry.json");
    fs::write(
        &payload,
        handoff_payload_with(r#""requested_outputs":["implementation_plan"]"#),
    )
    .unwrap();
    fs::write(
        &approval,
        human_approval(
            "approved",
            "sha256:0000000000000000000000000000000000000000000000000000000000000000",
            "2000-01-01T00:00:00Z",
        ),
    )
    .unwrap();
    fs::write(&registry, approval_key_registry(&[default_approval_key()])).unwrap();

    let out = Command::new(COSMATIC)
        .args([
            "handoff",
            "plan",
            payload.to_str().unwrap(),
            "--dry-run",
            "--json",
            "--human-approval",
            approval.to_str().unwrap(),
            "--approval-key-registry",
            registry.to_str().unwrap(),
        ])
        .output()
        .expect("spawn bolt-cosmatic handoff plan --human-approval");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(!out.status.success(), "expired approval must refuse");
    assert!(
        stdout.contains("human_approval_not_active"),
        "stdout:\n{stdout}"
    );
}

#[test]
fn handoff_plan_refuses_human_approval_subject_mismatch() {
    let tmp = tempfile::tempdir().unwrap();
    let payload = tmp.path().join("handoff.json");
    let approval = tmp.path().join("human-approval.json");
    let registry = tmp.path().join("approval-key-registry.json");
    fs::write(
        &payload,
        handoff_payload_with(r#""requested_outputs":["implementation_plan"]"#),
    )
    .unwrap();
    fs::write(
        &approval,
        human_approval(
            "approved",
            "sha256:1111111111111111111111111111111111111111111111111111111111111111",
            "2099-12-31T00:00:00Z",
        ),
    )
    .unwrap();
    fs::write(&registry, approval_key_registry(&[default_approval_key()])).unwrap();

    let out = Command::new(COSMATIC)
        .args([
            "handoff",
            "plan",
            payload.to_str().unwrap(),
            "--dry-run",
            "--json",
            "--human-approval",
            approval.to_str().unwrap(),
            "--approval-key-registry",
            registry.to_str().unwrap(),
        ])
        .output()
        .expect("spawn bolt-cosmatic handoff plan --human-approval");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(!out.status.success(), "mismatched approval must refuse");
    assert!(
        stdout.contains("human_approval_not_approved"),
        "stdout:\n{stdout}"
    );
}

#[test]
fn handoff_plan_refuses_invalid_human_approval_signature() {
    let tmp = tempfile::tempdir().unwrap();
    let payload = tmp.path().join("handoff.json");
    let approval = tmp.path().join("human-approval.json");
    let registry = tmp.path().join("approval-key-registry.json");
    fs::write(
        &payload,
        handoff_payload_with(r#""requested_outputs":["implementation_plan"]"#),
    )
    .unwrap();
    fs::write(
        &approval,
        human_approval(
            "approved",
            "sha256:0000000000000000000000000000000000000000000000000000000000000000",
            "2099-12-31T00:00:00Z",
        )
        .replacen("\"value\":\"ed25519:", "\"value\":\"ed25519:ff", 1),
    )
    .unwrap();
    fs::write(&registry, approval_key_registry(&[default_approval_key()])).unwrap();

    let out = Command::new(COSMATIC)
        .args([
            "handoff",
            "plan",
            payload.to_str().unwrap(),
            "--dry-run",
            "--json",
            "--human-approval",
            approval.to_str().unwrap(),
            "--approval-key-registry",
            registry.to_str().unwrap(),
        ])
        .output()
        .expect("spawn bolt-cosmatic handoff plan --human-approval");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(!out.status.success(), "invalid signature must refuse");
    assert!(
        stdout.contains("human_approval_signature_invalid"),
        "stdout:\n{stdout}"
    );
}

#[test]
fn handoff_plan_refuses_failed_wrench_evidence_report() {
    let tmp = tempfile::tempdir().unwrap();
    let payload = tmp.path().join("handoff.json");
    let evidence = tmp.path().join("wrench-evidence.json");
    fs::write(
        &payload,
        handoff_payload_with(r#""requested_outputs":["implementation_plan"]"#),
    )
    .unwrap();
    fs::write(&evidence, wrench_evidence_report("failed")).unwrap();

    let out = Command::new(COSMATIC)
        .args([
            "handoff",
            "plan",
            payload.to_str().unwrap(),
            "--dry-run",
            "--json",
            "--evidence-report",
            evidence.to_str().unwrap(),
        ])
        .output()
        .expect("spawn bolt-cosmatic handoff plan --evidence-report");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(!out.status.success(), "failed evidence must refuse");
    assert!(
        stdout.contains("wrench_evidence_not_passing"),
        "stdout:\n{stdout}"
    );
    assert!(stdout.contains("wrench_report_passed"), "stdout:\n{stdout}");
}

fn human_approval(decision: &str, subject_hash: &str, expires_at: &str) -> String {
    human_approval_with_key(
        decision,
        subject_hash,
        expires_at,
        "human-operator-demo-key-01",
        42,
    )
}

fn human_approval_with_key(
    decision: &str,
    subject_hash: &str,
    expires_at: &str,
    public_key_ref: &str,
    seed: u8,
) -> String {
    let approval_id = "approval_handoff_h_01";
    let subject_kind = "handoff_package";
    let subject_ref = "h";
    let approved_by = "operator-demo";
    let approved_at = "2026-07-02T00:00:00Z";
    let signing_key = SigningKey::from_bytes(&[seed; 32]);
    let signature_fields = HumanApprovalSignatureFields {
        approval_id,
        subject_kind,
        subject_ref,
        subject_hash,
        decision,
        approved_by,
        approved_at,
        expires_at,
    };
    let message = human_approval_signature_message(&signature_fields);
    let signature = format!(
        "ed25519:{}",
        lower_hex(&signing_key.sign(message.as_bytes()).to_bytes())
    );

    format!(
        r#"{{
          "format":"bolt.human_approval.v0.1",
          "approval_id":"{approval_id}",
          "subject":{{
            "kind":"{subject_kind}",
            "ref":"{subject_ref}",
            "hash":"{subject_hash}"
          }},
          "decision":"{decision}",
          "approved_by":"{approved_by}",
          "approved_at":"{approved_at}",
          "expires_at":"{expires_at}",
          "signature":{{
            "algorithm":"ed25519",
            "public_key_ref":"{public_key_ref}",
            "value":"{signature}"
          }}
        }}"#
    )
}

#[derive(Clone, Copy)]
struct ApprovalKeyFixture {
    public_key_ref: &'static str,
    seed: u8,
    state: &'static str,
    not_before: &'static str,
    expires_at: &'static str,
    rotated_from: Option<&'static str>,
    rotated_to: Option<&'static str>,
    revoked_at: Option<&'static str>,
}

fn default_approval_key() -> ApprovalKeyFixture {
    ApprovalKeyFixture {
        public_key_ref: "human-operator-demo-key-01",
        seed: 42,
        state: "active",
        not_before: "2000-01-01T00:00:00Z",
        expires_at: "2099-12-31T00:00:00Z",
        rotated_from: None,
        rotated_to: None,
        revoked_at: None,
    }
}

fn approval_key_registry(keys: &[ApprovalKeyFixture]) -> String {
    let key_refs = keys
        .iter()
        .map(|key| {
            let signing_key = SigningKey::from_bytes(&[key.seed; 32]);
            let public_key = format!(
                "ed25519:{}",
                lower_hex(&signing_key.verifying_key().to_bytes())
            );
            let rotated_from = optional_json_string("rotated_from", key.rotated_from);
            let rotated_to = optional_json_string("rotated_to", key.rotated_to);
            let revoked_at = optional_json_string("revoked_at", key.revoked_at);
            format!(
                r#"{{
                  "public_key_ref":"{}",
                  "algorithm":"ed25519",
                  "public_key":"{}",
                  "state":"{}",
                  "not_before":"{}",
                  "expires_at":"{}"{}{}{}
                }}"#,
                key.public_key_ref,
                public_key,
                key.state,
                key.not_before,
                key.expires_at,
                rotated_from,
                rotated_to,
                revoked_at,
            )
        })
        .collect::<Vec<_>>()
        .join(",");

    format!(
        r#"{{
          "format":"bolt.approval_key_registry.v0.1",
          "registry_id":"approval-keys-demo",
          "issued_at":"2026-07-02T00:00:00Z",
          "keys":[{key_refs}]
        }}"#
    )
}

fn optional_json_string(field: &str, value: Option<&str>) -> String {
    value
        .map(|value| format!(r#", "{field}":"{value}""#))
        .unwrap_or_default()
}

struct HumanApprovalSignatureFields<'a> {
    approval_id: &'a str,
    subject_kind: &'a str,
    subject_ref: &'a str,
    subject_hash: &'a str,
    decision: &'a str,
    approved_by: &'a str,
    approved_at: &'a str,
    expires_at: &'a str,
}

fn human_approval_signature_message(fields: &HumanApprovalSignatureFields<'_>) -> String {
    format!(
        "bolt.human_approval.v0.1\napproval_id:{}\nsubject.kind:{}\nsubject.ref:{}\nsubject.hash:{}\ndecision:{}\napproved_by:{}\napproved_at:{}\nexpires_at:{}",
        fields.approval_id,
        fields.subject_kind,
        fields.subject_ref,
        fields.subject_hash,
        fields.decision,
        fields.approved_by,
        fields.approved_at,
        fields.expires_at,
    )
}

fn lower_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    encoded
}

fn gear_wrench_evidence_manifest(status: &str, artifact_hash: &str, state: &str) -> String {
    format!(
        r#"{{
          "manifest_id":"manifest:wrench-portal-usage-1234567890abcdef",
          "artifact":{{
            "artifact_id":"artifact:wrench-portal-usage-1234567890abcdef",
            "artifact_type":"inspection_report",
            "producer":"wrench-inspect",
            "version":"0.1.0",
            "hash":"{artifact_hash}",
            "manifest_ref":"manifest:wrench-portal-usage-1234567890abcdef",
            "state":"{state}",
            "created_at":"2026-07-02T00:00:00Z"
          }},
          "package_type":"json_bundle",
          "checksums":[{{"algorithm":"sha256","value":"{artifact_hash}"}}],
          "provenance_id":"provenance:wrench-portal-usage-1234567890abcdef",
          "retention":{{"policy_ref":null,"expires_at":null,"revoked_at":null,"delete_after":null}},
          "distribution":{{"channels":[],"install_floor":null,"published_at":null}},
          "metadata":{{"values":{{
            "source_format":"wrench.evidence_report.v0.1",
            "evidence_status":"{status}",
            "source_report_hash":"sha256:4444444444444444444444444444444444444444444444444444444444444444",
            "subject_kind":"portal_ui",
            "subject_ref":"rumble-lm/crates/ui"
          }}}}
        }}"#
    )
}

fn wrench_evidence_report(status: &str) -> String {
    format!(
        r#"{{
          "format":"wrench.evidence_report.v0.1",
          "report_id":"wrench-portal-usage-1234567890abcdef",
          "generated_at":"2026-07-02T00:00:00Z",
          "producer":{{"name":"wrench-inspect","version":"0.1.0"}},
          "subject":{{"kind":"portal_ui","reference":"rumble-lm/crates/ui"}},
          "status":"{status}",
          "summary":{{"errors":0,"warnings":0,"infos":0}},
          "findings":[],
          "checks":[{{"code":"portal_tokens_present","status":"passed","summary":"tokens present"}}],
          "evidence_refs":[],
          "source_report":{{
            "format":"wrench.portal_usage_report.v0.1",
            "hash":"sha256:4444444444444444444444444444444444444444444444444444444444444444",
            "body":{{}}
          }},
          "next_actions":[]
        }}"#
    )
}

fn run_killed(
    args: &[&str],
    disable_var: &str,
    extra_env: &[(&str, &str)],
) -> std::process::Output {
    let tmp = tempfile::tempdir().unwrap();
    let mut c = Command::new(COSMATIC);
    c.args(args).env(disable_var, "1").current_dir(tmp.path());
    for (k, v) in extra_env {
        c.env(k, v);
    }
    c.output().expect("spawn bolt-cosmatic")
}

#[test]
fn loop_dry_run_works_without_github_token() {
    let tmp = tempfile::tempdir().unwrap();
    let out = Command::new(COSMATIC)
        .args([
            "loop",
            "--dry-run",
            "--issue",
            "3",
            "--title",
            "x",
            "--repo",
            "o/n",
        ])
        .env_remove("GITHUB_TOKEN")
        .env_remove("GH_TOKEN")
        .current_dir(tmp.path())
        .output()
        .expect("spawn bolt-cosmatic loop --dry-run");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "stdout:\n{stdout}\nstderr:\n{stderr}");
    assert!(
        stdout.contains("bolt/run/issue-3/issue-3/attempt-1"),
        "dry-run should show the structured branch name; stdout:\n{stdout}"
    );
    assert!(
        stdout.contains("skipped remote gate check: no GitHub token"),
        "dry-run without a token should skip remote gate checks, not fail; stdout:\n{stdout}"
    );
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
        "BOLT_COSMATIC_DISPATCH_DISABLED",
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
        "BOLT_COSMATIC_AUTOMERGE_DISABLED",
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
        "BOLT_COSMATIC_DEPLOY_DISABLED",
        &[
            ("BOLT_COSMATIC_DEPLOY_CANARY", "true"),
            ("BOLT_COSMATIC_DEPLOY_PROMOTE", "true"),
            ("BOLT_COSMATIC_DEPLOY_ROLLBACK", "true"),
            ("BOLT_COSMATIC_DEPLOY_SMOKE", "true"),
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
        "BOLT_COSMATIC_LOOP_DISABLED",
        &[("GITHUB_TOKEN", "ghp_test_dummy")],
    );
    assert_refused(&out, "loop");
}
