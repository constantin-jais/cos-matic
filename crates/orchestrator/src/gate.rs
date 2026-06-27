//! The gate-wall: run the configured hard-gate checks and decide pass/fail.

use std::path::PathBuf;
use std::process::Command;

use crate::goals::{Goals, Metrics, Status, status};

/// Runs named checks (`"fmt"`, `"clippy"`, `"tests"`). Abstracted so the gate
/// engine is testable without shelling out to `cargo`.
pub trait CheckRunner {
    /// Run a named check; `true` when it passed.
    fn run_check(&self, check: &str) -> bool;
}

/// The checks the gate-wall knows how to run, in report order.
pub const CHECKS: [&str; 3] = ["fmt", "clippy", "tests"];

/// The metric a given check feeds (a violation count: `0` means pass).
pub fn metric_for(check: &str) -> &'static str {
    match check {
        "fmt" => "fmt_violations",
        "clippy" => "clippy_violations",
        "tests" => "tests_failed",
        _ => "unknown_violations",
    }
}

/// The `cargo` arguments for a named check (an empty slice for an unknown one).
pub fn check_command(check: &str) -> &'static [&'static str] {
    match check {
        "fmt" => &["fmt", "--all", "--check"],
        "clippy" => &[
            "clippy",
            "--workspace",
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ],
        "tests" => &["test", "--workspace"],
        _ => &[],
    }
}

/// Run every known check and turn the outcomes into metric values
/// (`0.0` when the check passed, `1.0` when it failed).
pub fn collect_metrics(runner: &dyn CheckRunner) -> Metrics {
    let mut metrics = Metrics::new();
    for check in CHECKS {
        let value = if runner.run_check(check) { 0.0 } else { 1.0 };
        metrics.insert(metric_for(check).to_string(), value);
    }
    metrics
}

/// The result of running the gate-wall against a `goals.toml`.
pub struct GateReport {
    /// One `(gate name, status)` per hard gate, in declaration order.
    pub rows: Vec<(String, Status)>,
    /// `true` iff every hard gate is [`Status::Green`] (fail-closed: a
    /// [`Status::Pending`] hard gate — metric unavailable — does not pass).
    pub all_green: bool,
    /// The full Markdown report (phase, milestones, gates, observability).
    pub markdown: String,
}

/// Collect metrics via `runner`, evaluate every hard gate, assemble a report.
pub fn run(goals: &Goals, runner: &dyn CheckRunner) -> GateReport {
    let metrics = collect_metrics(runner);
    let rows: Vec<(String, Status)> = goals
        .gate
        .iter()
        .map(|g| {
            (
                g.name.clone(),
                status(&g.metric, g.op, g.threshold, &metrics),
            )
        })
        .collect();
    let all_green = rows.iter().all(|(_, s)| *s == Status::Green);
    let markdown = crate::goals::render_markdown(goals, &metrics);
    GateReport {
        rows,
        all_green,
        markdown,
    }
}

/// The real [`CheckRunner`]: shells out to `cargo` in `repo_root`.
pub struct CargoRunner {
    /// The directory `cargo` is invoked in.
    pub repo_root: PathBuf,
}

impl CheckRunner for CargoRunner {
    fn run_check(&self, check: &str) -> bool {
        let args = check_command(check);
        if args.is_empty() {
            return false;
        }
        Command::new("cargo")
            .args(args)
            .current_dir(&self.repo_root)
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::goals::{Goals, Status, parse};

    /// A runner that passes every check except an optional named one.
    struct FakeRunner {
        fail: Option<&'static str>,
    }

    impl CheckRunner for FakeRunner {
        fn run_check(&self, check: &str) -> bool {
            self.fail != Some(check)
        }
    }

    fn goals_fixture() -> Goals {
        parse(
            r#"
[phase]
id = "A1"
title = "Goals & gates"
[[gate]]
name = "fmt"
metric = "fmt_violations"
op = "eq"
threshold = 0
[[gate]]
name = "clippy"
metric = "clippy_violations"
op = "eq"
threshold = 0
[[gate]]
name = "tests"
metric = "tests_failed"
op = "eq"
threshold = 0
"#,
        )
        .unwrap()
    }

    #[test]
    fn all_pass_is_all_green() {
        let report = run(&goals_fixture(), &FakeRunner { fail: None });
        assert!(report.all_green);
        assert!(report.rows.iter().all(|(_, s)| *s == Status::Green));
    }

    #[test]
    fn one_failed_check_is_red_and_not_all_green() {
        let report = run(
            &goals_fixture(),
            &FakeRunner {
                fail: Some("clippy"),
            },
        );
        assert!(!report.all_green);
        let clippy = report.rows.iter().find(|(n, _)| n == "clippy").unwrap();
        assert_eq!(clippy.1, Status::Red);
        let fmt = report.rows.iter().find(|(n, _)| n == "fmt").unwrap();
        assert_eq!(fmt.1, Status::Green);
    }

    #[test]
    fn collect_metrics_maps_checks_to_violation_counts() {
        let m = collect_metrics(&FakeRunner {
            fail: Some("tests"),
        });
        assert_eq!(m["fmt_violations"], 0.0);
        assert_eq!(m["clippy_violations"], 0.0);
        assert_eq!(m["tests_failed"], 1.0);
    }

    #[test]
    fn check_command_maps_known_and_unknown() {
        assert_eq!(
            check_command("fmt").to_vec(),
            vec!["fmt", "--all", "--check"]
        );
        assert_eq!(check_command("tests").to_vec(), vec!["test", "--workspace"]);
        assert!(check_command("nope").is_empty());
    }
}
