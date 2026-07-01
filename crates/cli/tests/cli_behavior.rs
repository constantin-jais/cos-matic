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
