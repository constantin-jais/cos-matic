//! `goals.toml` — declarative phase, milestones, hard gates and observability.
//!
//! Thresholds are integers (sufficient for A1: violation counts and a coverage
//! percentage); runtime metric *values* are `f64` so a metric like coverage can
//! be fractional. Comparisons happen in `f64` (see [`evaluate`]).

use serde::Deserialize;

/// Comparison operator for a gate or observability metric.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Op {
    Eq,
    Ne,
    Lt,
    Lte,
    Gt,
    Gte,
}

/// The phase the project is currently in.
#[derive(Debug, Clone, Deserialize)]
pub struct Phase {
    pub id: String,
    pub title: String,
}

/// A sub-milestone within a phase.
#[derive(Debug, Clone, Deserialize)]
pub struct Milestone {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub done: bool,
}

/// A hard, blocking gate: `metric op threshold` must hold for the phase to pass.
#[derive(Debug, Clone, Deserialize)]
pub struct Gate {
    pub name: String,
    pub metric: String,
    pub op: Op,
    pub threshold: i64,
}

/// A non-blocking observability target (reported, never blocks).
#[derive(Debug, Clone, Deserialize)]
pub struct Observe {
    pub name: String,
    pub metric: String,
    pub op: Op,
    pub threshold: i64,
}

/// The whole `goals.toml`.
#[derive(Debug, Clone, Deserialize)]
pub struct Goals {
    pub phase: Phase,
    #[serde(default)]
    pub milestone: Vec<Milestone>,
    #[serde(default)]
    pub gate: Vec<Gate>,
    #[serde(default)]
    pub observe: Vec<Observe>,
}

/// Failure to parse a `goals.toml`.
#[derive(Debug)]
pub struct GoalsError(String);

impl std::fmt::Display for GoalsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid goals.toml: {}", self.0)
    }
}

impl std::error::Error for GoalsError {}

/// Parse a `goals.toml` source string into a [`Goals`].
pub fn parse(src: &str) -> Result<Goals, GoalsError> {
    toml::from_str(src).map_err(|e| GoalsError(e.to_string()))
}

/// Live metric values keyed by name (e.g. `"fmt_violations" -> 0.0`).
pub type Metrics = std::collections::BTreeMap<String, f64>;

/// Verdict for a gate or observability row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// The metric is present and satisfies the comparison.
    Green,
    /// The metric is present and violates the comparison.
    Red,
    /// The metric is unavailable; nothing was evaluated.
    Pending,
}

/// Apply a comparison operator: does `actual op threshold` hold?
pub fn evaluate(op: Op, actual: f64, threshold: f64) -> bool {
    match op {
        Op::Eq => actual == threshold,
        Op::Ne => actual != threshold,
        Op::Lt => actual < threshold,
        Op::Lte => actual <= threshold,
        Op::Gt => actual > threshold,
        Op::Gte => actual >= threshold,
    }
}

/// Evaluate one metric against an operator/threshold given live metrics.
///
/// Returns [`Status::Pending`] when the metric is absent.
pub fn status(metric: &str, op: Op, threshold: i64, metrics: &Metrics) -> Status {
    match metrics.get(metric) {
        None => Status::Pending,
        Some(&actual) if evaluate(op, actual, threshold as f64) => Status::Green,
        Some(_) => Status::Red,
    }
}

fn glyph(s: Status) -> &'static str {
    match s {
        Status::Green => "✅",
        Status::Red => "🔴",
        Status::Pending => "⏳",
    }
}

fn op_symbol(op: Op) -> &'static str {
    match op {
        Op::Eq => "==",
        Op::Ne => "!=",
        Op::Lt => "<",
        Op::Lte => "<=",
        Op::Gt => ">",
        Op::Gte => ">=",
    }
}

/// Render a Markdown report of the goals evaluated against live `metrics`.
///
/// Hard gates and observability rows show `✅` / `🔴` / `⏳` (pending when the
/// metric is unavailable). Pure: no I/O, deterministic for a given input.
pub fn render_markdown(goals: &Goals, metrics: &Metrics) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    let _ = writeln!(out, "# Phase {} — {}\n", goals.phase.id, goals.phase.title);

    if !goals.milestone.is_empty() {
        let _ = writeln!(out, "## Milestones\n");
        for m in &goals.milestone {
            let check = if m.done { 'x' } else { ' ' };
            let _ = writeln!(out, "- [{check}] {}", m.title);
        }
        out.push('\n');
    }

    if !goals.gate.is_empty() {
        let _ = writeln!(out, "## Hard gates\n");
        let _ = writeln!(out, "| Gate | Metric | Rule | Status |");
        let _ = writeln!(out, "| --- | --- | --- | --- |");
        for g in &goals.gate {
            let s = status(&g.metric, g.op, g.threshold, metrics);
            let _ = writeln!(
                out,
                "| {} | `{}` | `{} {}` | {} |",
                g.name,
                g.metric,
                op_symbol(g.op),
                g.threshold,
                glyph(s)
            );
        }
        out.push('\n');
    }

    if !goals.observe.is_empty() {
        let _ = writeln!(out, "## Observability\n");
        let _ = writeln!(out, "| Metric | Rule | Status |");
        let _ = writeln!(out, "| --- | --- | --- |");
        for o in &goals.observe {
            let s = status(&o.metric, o.op, o.threshold, metrics);
            let _ = writeln!(
                out,
                "| {} (`{}`) | `{} {}` | {} |",
                o.name,
                o.metric,
                op_symbol(o.op),
                o.threshold,
                glyph(s)
            );
        }
        out.push('\n');
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_phase_gate_and_observe() {
        let src = r#"
[phase]
id = "A1"
title = "Goals & gates"

[[milestone]]
id = "gate-wall"
title = "aom gate run enforces fmt+clippy+tests"
done = true

[[gate]]
name = "fmt"
metric = "fmt_violations"
op = "eq"
threshold = 0

[[observe]]
name = "coverage"
metric = "coverage_pct"
op = "gte"
threshold = 80
"#;
        let goals = parse(src).expect("valid goals.toml");
        assert_eq!(goals.phase.id, "A1");
        assert_eq!(goals.phase.title, "Goals & gates");
        assert_eq!(goals.milestone.len(), 1);
        assert!(goals.milestone[0].done);
        assert_eq!(goals.gate.len(), 1);
        assert_eq!(goals.gate[0].name, "fmt");
        assert_eq!(goals.gate[0].metric, "fmt_violations");
        assert_eq!(goals.gate[0].op, Op::Eq);
        assert_eq!(goals.gate[0].threshold, 0);
        assert_eq!(goals.observe[0].op, Op::Gte);
        assert_eq!(goals.observe[0].threshold, 80);
    }

    #[test]
    fn rejects_invalid_toml() {
        assert!(parse("this is = not valid = toml").is_err());
    }

    #[test]
    fn evaluate_covers_all_ops() {
        assert!(evaluate(Op::Eq, 0.0, 0.0));
        assert!(!evaluate(Op::Eq, 1.0, 0.0));
        assert!(evaluate(Op::Ne, 1.0, 0.0));
        assert!(!evaluate(Op::Ne, 2.0, 2.0));
        assert!(evaluate(Op::Lt, 1.0, 2.0));
        assert!(!evaluate(Op::Lt, 2.0, 2.0));
        assert!(evaluate(Op::Lte, 2.0, 2.0));
        assert!(evaluate(Op::Gt, 3.0, 2.0));
        assert!(evaluate(Op::Gte, 80.0, 80.0));
        assert!(!evaluate(Op::Gte, 79.0, 80.0));
    }

    #[test]
    fn status_is_pending_when_metric_absent() {
        let m = Metrics::new();
        assert_eq!(status("coverage_pct", Op::Gte, 80, &m), Status::Pending);
    }

    #[test]
    fn status_green_and_red() {
        let mut m = Metrics::new();
        m.insert("fmt_violations".to_string(), 0.0);
        m.insert("coverage_pct".to_string(), 70.0);
        assert_eq!(status("fmt_violations", Op::Eq, 0, &m), Status::Green);
        assert_eq!(status("coverage_pct", Op::Gte, 80, &m), Status::Red);
    }

    #[test]
    fn renders_phase_gates_and_observability() {
        let goals = parse(
            r#"
[phase]
id = "A1"
title = "Goals & gates"
[[milestone]]
id = "m1"
title = "done thing"
done = true
[[milestone]]
id = "m2"
title = "todo thing"
done = false
[[gate]]
name = "fmt"
metric = "fmt_violations"
op = "eq"
threshold = 0
[[observe]]
name = "coverage"
metric = "coverage_pct"
op = "gte"
threshold = 80
"#,
        )
        .unwrap();
        let mut m = Metrics::new();
        m.insert("fmt_violations".to_string(), 0.0); // coverage_pct absent → Pending
        let md = render_markdown(&goals, &m);
        assert!(md.contains("# Phase A1 — Goals & gates"), "{md}");
        assert!(md.contains("- [x] done thing"), "{md}");
        assert!(md.contains("- [ ] todo thing"), "{md}");
        assert!(md.contains("## Hard gates"), "{md}");
        assert!(md.contains("fmt"), "{md}");
        assert!(md.contains("✅"), "{md}"); // fmt is green
        assert!(md.contains("## Observability"), "{md}");
        assert!(md.contains("⏳"), "{md}"); // coverage is pending
    }
}
