//! End-to-end: a real project on disk, exercised through the public API, proving
//! the full pipeline plus the safe-write / drift guarantees.

use std::fs;
use std::path::Path;

use agent_o_matic::Error;
use agent_o_matic::generate::{self, Action, Options};

fn opts(manifest: &Path, check: bool, force: bool) -> Options {
    Options {
        manifest_path: manifest.to_path_buf(),
        check,
        force,
    }
}

fn setup(root: &Path) {
    fs::create_dir_all(root.join("domains")).unwrap();
    fs::write(
        root.join("domains/code-style.md"),
        "# Code Style\n- Explicit > implicit.\n",
    )
    .unwrap();
    fs::write(
        root.join("domains/security.md"),
        "# Security\n- Validate all input.\n",
    )
    .unwrap();
    fs::write(
        root.join("harness.toml"),
        r#"
[package]
name = "demo"

[[domains]]
name = "code-style"
priority = 5
content_file = "domains/code-style.md"

[[domains]]
name = "security"
priority = 10
content_file = "domains/security.md"

[[profiles]]
name = "default"
domains = ["code-style", "security"]

[[targets]]
name = "agents-md"
adapter = "universal"
output_file = "AGENTS.md"
profile = "default"
"#,
    )
    .unwrap();
}

#[test]
fn full_pipeline_is_deterministic_idempotent_and_clobber_safe() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    setup(root);
    let manifest = root.join("harness.toml");

    // 1. First run creates AGENTS.md, domains ordered by priority (security first).
    let r1 = generate::run(&opts(&manifest, false, false)).unwrap();
    assert_eq!(r1.files.len(), 1);
    assert_eq!(r1.files[0].action, Action::Created);
    let agents = fs::read_to_string(root.join("AGENTS.md")).unwrap();
    assert_eq!(
        agents,
        "# Security\n- Validate all input.\n\n# Code Style\n- Explicit > implicit.\n"
    );
    assert!(root.join(".harness/lock.toml").exists());
    assert!(root.join(".harness/audit.jsonl").exists());

    // 2. Re-running without changes is a no-op.
    let r2 = generate::run(&opts(&manifest, false, false)).unwrap();
    assert_eq!(r2.files[0].action, Action::Unchanged);

    // 3. --check passes while up to date.
    generate::run(&opts(&manifest, true, false)).expect("no drift when up to date");

    // 4. A human edit is never silently clobbered.
    fs::write(root.join("AGENTS.md"), "human edit\n").unwrap();
    let err = generate::run(&opts(&manifest, false, false)).unwrap_err();
    assert!(matches!(err, Error::Clobber { .. }));
    assert_eq!(
        fs::read_to_string(root.join("AGENTS.md")).unwrap(),
        "human edit\n"
    );

    // 5. --check reports drift after the edit.
    let drift = generate::run(&opts(&manifest, true, false)).unwrap_err();
    assert!(matches!(drift, Error::Drift { .. }));

    // 6. --force re-adopts and regenerates.
    let r6 = generate::run(&opts(&manifest, false, true)).unwrap();
    assert_eq!(r6.files[0].action, Action::Updated);
    assert_eq!(fs::read_to_string(root.join("AGENTS.md")).unwrap(), agents);
}

#[test]
fn missing_manifest_reports_io_error() {
    let tmp = tempfile::tempdir().unwrap();
    let manifest = tmp.path().join("harness.toml"); // never created
    let err = generate::run(&opts(&manifest, false, false)).unwrap_err();
    assert!(matches!(err, Error::Io { .. }), "got {err:?}");
}

#[test]
fn includes_merge_domains_across_files_in_priority_order() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    fs::write(
        root.join("lib.toml"),
        "[package]\nname=\"lib\"\n[[domains]]\nname=\"sec\"\npriority=10\ncontent=\"SEC\"\n",
    )
    .unwrap();
    fs::write(
        root.join("harness.toml"),
        r#"
[package]
name = "demo"
[[includes]]
path = "lib.toml"
[[domains]]
name = "style"
priority = 1
content = "STYLE"
[[profiles]]
name = "default"
domains = ["style", "sec"]
[[targets]]
name = "agents-md"
adapter = "universal"
output_file = "AGENTS.md"
profile = "default"
"#,
    )
    .unwrap();
    let manifest = root.join("harness.toml");
    generate::run(&opts(&manifest, false, false)).unwrap();
    // sec (priority 10, from the included file) renders before style (priority 1).
    assert_eq!(
        fs::read_to_string(root.join("AGENTS.md")).unwrap(),
        "SEC\n\nSTYLE\n"
    );
}

#[test]
fn multi_target_generation_with_graceful_degradation() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    fs::write(
        root.join("harness.toml"),
        r#"
[package]
name = "multi"

[[domains]]
name = "general"
priority = 5
content = "GENERAL"

[[domains]]
name = "rust"
priority = 10
content = "RUST"
globs = ["src/**/*.rs"]

[[profiles]]
name = "default"
domains = ["general", "rust"]

[[targets]]
name = "agents"
adapter = "universal"
output_file = "AGENTS.md"
profile = "default"

[[targets]]
name = "claude"
adapter = "claude"
output_file = "CLAUDE.md"
profile = "default"

[[targets]]
name = "cursor"
adapter = "cursor"
output_dir = ".cursor/rules"
profile = "default"
"#,
    )
    .unwrap();
    let manifest = root.join("harness.toml");
    let report = generate::run(&opts(&manifest, false, false)).unwrap();

    // One run produced four files across three targets.
    assert!(root.join("AGENTS.md").exists());
    assert!(root.join("CLAUDE.md").exists());
    assert!(root.join(".cursor/rules/general.mdc").exists());
    assert!(root.join(".cursor/rules/rust.mdc").exists());

    // Cursor honors the glob: the rust rule is scoped, not always-on.
    let rust_mdc = fs::read_to_string(root.join(".cursor/rules/rust.mdc")).unwrap();
    assert!(rust_mdc.contains("globs: src/**/*.rs"));
    assert!(rust_mdc.contains("alwaysApply: false"));

    // universal and claude cannot express glob activation -> two warnings, but the
    // content is still rendered (graceful degradation, not an error).
    let glob_warnings = report
        .warnings
        .iter()
        .filter(|w| w.contains("glob activation"))
        .count();
    assert_eq!(glob_warnings, 2);

    // Whole multi-target set is idempotent on re-run.
    let r2 = generate::run(&opts(&manifest, false, false)).unwrap();
    assert!(r2.files.iter().all(|f| f.action == Action::Unchanged));
}

#[test]
fn two_targets_resolving_to_the_same_path_are_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    fs::write(
        root.join("harness.toml"),
        r#"
[package]
name = "dup"
[[domains]]
name = "a"
content = "A"
[[profiles]]
name = "default"
domains = ["a"]
[[targets]]
name = "cursor1"
adapter = "cursor"
output_dir = ".cursor/rules"
profile = "default"
[[targets]]
name = "cursor2"
adapter = "cursor"
output_dir = ".cursor/rules"
profile = "default"
"#,
    )
    .unwrap();
    let manifest = root.join("harness.toml");
    let err = generate::run(&opts(&manifest, false, false)).unwrap_err();
    assert!(
        matches!(err, Error::DuplicateRenderedPath { .. }),
        "got {err:?}"
    );
}

#[test]
fn cursor_partial_clobber_refuses_only_the_edited_file() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    fs::write(
        root.join("harness.toml"),
        r#"
[package]
name = "pc"
[[domains]]
name = "alpha"
content = "A"
[[domains]]
name = "beta"
content = "B"
[[profiles]]
name = "default"
domains = ["alpha", "beta"]
[[targets]]
name = "cursor"
adapter = "cursor"
output_dir = ".cursor/rules"
profile = "default"
"#,
    )
    .unwrap();
    let manifest = root.join("harness.toml");
    generate::run(&opts(&manifest, false, false)).unwrap();

    // Hand-edit one of the two generated rule files.
    fs::write(root.join(".cursor/rules/alpha.mdc"), "tampered\n").unwrap();

    // Regeneration refuses because alpha.mdc diverged from the lock; beta.mdc,
    // unchanged and tool-owned, is still present (per-file safe-write).
    let err = generate::run(&opts(&manifest, false, false)).unwrap_err();
    assert!(matches!(err, Error::Clobber { .. }), "got {err:?}");
    assert!(root.join(".cursor/rules/beta.mdc").exists());
}

#[test]
fn builtins_inject_embedded_library_content() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    fs::write(
        root.join("harness.toml"),
        r#"
[package]
name = "lib-demo"
builtins = ["four-axes"]

[[profiles]]
name = "default"
domains = ["four-axes"]

[[targets]]
name = "agents"
adapter = "universal"
output_file = "AGENTS.md"
profile = "default"
"#,
    )
    .unwrap();
    let manifest = root.join("harness.toml");
    generate::run(&opts(&manifest, false, false)).unwrap();
    // The built-in domain's content (authored only inside the binary) appears.
    let agents = fs::read_to_string(root.join("AGENTS.md")).unwrap();
    assert!(agents.contains("Decision Axes"), "got: {agents}");
}

#[test]
fn user_domain_colliding_with_a_builtin_name_is_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    fs::write(
        root.join("harness.toml"),
        r#"
[package]
name = "clash"
builtins = ["tdd"]

[[domains]]
name = "tdd"
content = "my own tdd"

[[profiles]]
name = "default"
domains = ["tdd"]

[[targets]]
name = "agents"
adapter = "universal"
output_file = "AGENTS.md"
profile = "default"
"#,
    )
    .unwrap();
    let manifest = root.join("harness.toml");
    let err = generate::run(&opts(&manifest, false, false)).unwrap_err();
    assert!(matches!(err, Error::DuplicateName { .. }), "got {err:?}");
}

#[test]
fn multiple_builtins_render_in_priority_order() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    fs::write(
        root.join("harness.toml"),
        r#"
[package]
name = "ordered"
builtins = ["code-style", "security-baseline"]

[[profiles]]
name = "default"
domains = ["code-style", "security-baseline"]

[[targets]]
name = "agents"
adapter = "universal"
output_file = "AGENTS.md"
profile = "default"
"#,
    )
    .unwrap();
    let manifest = root.join("harness.toml");
    generate::run(&opts(&manifest, false, false)).unwrap();
    // security-baseline (priority 90) must render before code-style (priority 70),
    // regardless of declaration order.
    let agents = fs::read_to_string(root.join("AGENTS.md")).unwrap();
    let sec = agents
        .find("# Security Baseline")
        .expect("security present");
    let style = agents.find("# Code Style").expect("code-style present");
    assert!(sec < style, "security-baseline should precede code-style");
}

const GATED_MANIFEST: &str = r#"
[package]
name = "gated"
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
kind = "KIND"
check = "no-dead-domains"
"#;

#[test]
fn hard_gate_failure_blocks_generation_and_writes_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    fs::write(
        root.join("harness.toml"),
        GATED_MANIFEST.replace("KIND", "hard_gate"),
    )
    .unwrap();
    let manifest = root.join("harness.toml");
    let err = generate::run(&opts(&manifest, false, false)).unwrap_err();
    match err {
        Error::GoalsFailed { failures } => {
            assert_eq!(failures.len(), 1);
            assert!(failures[0].contains("no-dead-domains"), "got {failures:?}");
            assert!(
                failures[0].contains("orphan"),
                "should name the dead domain"
            );
        }
        other => panic!("expected GoalsFailed, got {other:?}"),
    }
    // A failed hard gate aborts before writing any output.
    assert!(!root.join("AGENTS.md").exists());
}

#[test]
fn unknown_check_in_a_goal_is_rejected_and_writes_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    fs::write(
        root.join("harness.toml"),
        r#"
[package]
name = "badcheck"
[[domains]]
name = "a"
content = "A"
[[profiles]]
name = "default"
domains = ["a"]
[[targets]]
name = "agents"
adapter = "universal"
output_file = "AGENTS.md"
profile = "default"
[[goals]]
kind = "hard_gate"
check = "does-not-exist"
"#,
    )
    .unwrap();
    let manifest = root.join("harness.toml");
    let err = generate::run(&opts(&manifest, false, false)).unwrap_err();
    assert!(matches!(err, Error::UnknownCheck { .. }), "got {err:?}");
    assert!(!root.join("AGENTS.md").exists());
}

#[test]
fn observability_goal_reports_without_blocking() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    fs::write(
        root.join("harness.toml"),
        GATED_MANIFEST.replace("KIND", "observability"),
    )
    .unwrap();
    let manifest = root.join("harness.toml");
    let report = generate::run(&opts(&manifest, false, false)).unwrap();
    // Generation succeeded despite the failing observability goal,
    assert!(root.join("AGENTS.md").exists());
    // and the failing outcome is still reported.
    assert_eq!(report.goals.len(), 1);
    assert!(!report.goals[0].passed);
}
