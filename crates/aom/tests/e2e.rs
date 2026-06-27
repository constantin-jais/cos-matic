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
