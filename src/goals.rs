//! Goals: safe, declarative checks over the configuration (ADR-0009).
//!
//! Every check is a pure function of the [`ConfigTree`] — no shell, no I/O. A
//! `hard_gate` goal blocks the run when its check fails; an `observability` goal
//! is only reported. The checks operate at the *config* level (a profile's merged
//! content is known before rendering), so they run before any file is written.

use crate::config::schema::{Goal, GoalKind};
use crate::error::{Error, Result};
use crate::ir::{ConfigTree, ResolvedDomain};
use crate::render::concatenate;

/// The supported check ids, single source of truth.
pub const CHECK_IDS: &[&str] = &["no-dead-domains", "require-domains", "max-content-lines"];

/// A check, parsed from its id so dispatch is exhaustive and typo-proof.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Check {
    NoDeadDomains,
    RequireDomains,
    MaxContentLines,
}

impl Check {
    fn parse(id: &str) -> Option<Self> {
        match id {
            "no-dead-domains" => Some(Check::NoDeadDomains),
            "require-domains" => Some(Check::RequireDomains),
            "max-content-lines" => Some(Check::MaxContentLines),
            _ => None,
        }
    }
}

/// The result of evaluating one goal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoalOutcome {
    pub kind: GoalKind,
    pub check: String,
    pub passed: bool,
    pub detail: String,
}

impl GoalOutcome {
    /// A failed hard gate is what blocks a run.
    pub fn is_blocking_failure(&self) -> bool {
        self.kind == GoalKind::HardGate && !self.passed
    }
}

/// Evaluate every goal against the configuration.
pub fn evaluate(tree: &ConfigTree, goals: &[Goal]) -> Result<Vec<GoalOutcome>> {
    goals.iter().map(|g| evaluate_one(tree, g)).collect()
}

fn evaluate_one(tree: &ConfigTree, goal: &Goal) -> Result<GoalOutcome> {
    let check = Check::parse(&goal.check).ok_or_else(|| Error::UnknownCheck {
        check: goal.check.clone(),
    })?;

    // Reject misconfigured goals: a check missing its required parameter would
    // silently pass, turning a hard gate into a no-op (the review's footgun).
    let (passed, detail) = match check {
        Check::NoDeadDomains => check_no_dead_domains(tree),
        Check::RequireDomains => {
            let domains = goal.domains.as_deref().unwrap_or(&[]);
            if domains.is_empty() {
                return Err(Error::InvalidGoal {
                    check: goal.check.clone(),
                    reason: "needs a non-empty `domains` list".to_string(),
                });
            }
            check_require_domains(tree, domains)
        }
        Check::MaxContentLines => {
            if goal.kind == GoalKind::HardGate && goal.max.is_none() {
                return Err(Error::InvalidGoal {
                    check: goal.check.clone(),
                    reason: "a hard_gate needs a `max` threshold to enforce".to_string(),
                });
            }
            check_max_content_lines(tree, goal.max)
        }
    };

    Ok(GoalOutcome {
        kind: goal.kind,
        check: goal.check.clone(),
        passed,
        detail,
    })
}

/// Every domain must be selected by at least one profile.
fn check_no_dead_domains(tree: &ConfigTree) -> (bool, String) {
    let used: std::collections::HashSet<&str> = tree
        .profiles
        .iter()
        .flat_map(|p| p.domains.iter().map(String::as_str))
        .collect();
    let dead: Vec<&str> = tree
        .domains
        .iter()
        .map(|d| d.name.as_str())
        .filter(|n| !used.contains(n))
        .collect();
    if dead.is_empty() {
        (
            true,
            format!("all {} domain(s) are used by a profile", tree.domains.len()),
        )
    } else {
        (
            false,
            format!("dead domains (used by no profile): {}", dead.join(", ")),
        )
    }
}

/// Every named domain must exist.
fn check_require_domains(tree: &ConfigTree, required: &[String]) -> (bool, String) {
    let have: std::collections::HashSet<&str> =
        tree.domains.iter().map(|d| d.name.as_str()).collect();
    let missing: Vec<&str> = required
        .iter()
        .map(String::as_str)
        .filter(|n| !have.contains(n))
        .collect();
    if missing.is_empty() {
        (
            true,
            format!("all {} required domain(s) present", required.len()),
        )
    } else {
        (
            false,
            format!("missing required domains: {}", missing.join(", ")),
        )
    }
}

/// The largest single rendered file (a profile's merged content) must stay within
/// `max` lines. The count uses the real renderer (`concatenate`) so blank-line
/// separators are included — matching the bytes actually written. Without a
/// `max`, the metric is only reported (an observability use).
fn check_max_content_lines(tree: &ConfigTree, max: Option<i64>) -> (bool, String) {
    let largest = tree
        .profiles
        .iter()
        .map(|p| {
            let domains: Vec<&ResolvedDomain> =
                p.domains.iter().filter_map(|n| tree.domain(n)).collect();
            concatenate(&domains).lines().count()
        })
        .max()
        .unwrap_or(0);
    match max {
        Some(m) => (
            largest as i64 <= m,
            format!("largest profile renders to {largest} line(s) (max {m})"),
        ),
        None => (
            true,
            format!("largest profile renders to {largest} line(s)"),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::schema::Profile;

    fn domain(name: &str, content: &str) -> ResolvedDomain {
        ResolvedDomain {
            name: name.to_string(),
            priority: 0,
            content: content.to_string(),
            globs: None,
        }
    }

    fn tree(domains: Vec<ResolvedDomain>, profiles: Vec<(&str, Vec<&str>)>) -> ConfigTree {
        ConfigTree {
            domains,
            profiles: profiles
                .into_iter()
                .map(|(name, ds)| Profile {
                    name: name.to_string(),
                    domains: ds.into_iter().map(str::to_string).collect(),
                })
                .collect(),
            targets: vec![],
        }
    }

    fn goal(kind: GoalKind, check: &str, max: Option<i64>, domains: Option<Vec<&str>>) -> Goal {
        Goal {
            kind,
            check: check.to_string(),
            max,
            domains: domains.map(|v| v.into_iter().map(str::to_string).collect()),
        }
    }

    #[test]
    fn all_check_ids_parse() {
        for id in CHECK_IDS {
            assert!(Check::parse(id).is_some(), "{id} must parse");
        }
    }

    #[test]
    fn no_dead_domains_passes_when_all_used() {
        let t = tree(vec![domain("a", "x")], vec![("p", vec!["a"])]);
        let out = evaluate(
            &t,
            &[goal(GoalKind::HardGate, "no-dead-domains", None, None)],
        )
        .unwrap();
        assert!(out[0].passed);
    }

    #[test]
    fn no_dead_domains_fails_on_unused_domain() {
        let t = tree(
            vec![domain("a", "x"), domain("orphan", "y")],
            vec![("p", vec!["a"])],
        );
        let out = evaluate(
            &t,
            &[goal(GoalKind::HardGate, "no-dead-domains", None, None)],
        )
        .unwrap();
        assert!(!out[0].passed);
        assert!(out[0].detail.contains("orphan"));
        assert!(out[0].is_blocking_failure());
    }

    #[test]
    fn require_domains_reports_missing() {
        let t = tree(vec![domain("a", "x")], vec![("p", vec!["a"])]);
        let out = evaluate(
            &t,
            &[goal(
                GoalKind::HardGate,
                "require-domains",
                None,
                Some(vec!["a", "security-baseline"]),
            )],
        )
        .unwrap();
        assert!(!out[0].passed);
        assert!(out[0].detail.contains("security-baseline"));
    }

    #[test]
    fn require_domains_without_domains_is_a_misconfiguration() {
        let t = tree(vec![domain("a", "x")], vec![("p", vec!["a"])]);
        let err = evaluate(
            &t,
            &[goal(GoalKind::HardGate, "require-domains", None, None)],
        )
        .unwrap_err();
        assert!(matches!(err, Error::InvalidGoal { .. }), "got {err:?}");
    }

    #[test]
    fn hard_gate_max_content_lines_without_max_is_a_misconfiguration() {
        let t = tree(vec![domain("a", "x")], vec![("p", vec!["a"])]);
        let err = evaluate(
            &t,
            &[goal(GoalKind::HardGate, "max-content-lines", None, None)],
        )
        .unwrap_err();
        assert!(matches!(err, Error::InvalidGoal { .. }), "got {err:?}");
    }

    #[test]
    fn observability_max_content_lines_without_max_just_reports() {
        let t = tree(vec![domain("a", "l1\nl2\n")], vec![("p", vec!["a"])]);
        let out = evaluate(
            &t,
            &[goal(
                GoalKind::Observability,
                "max-content-lines",
                None,
                None,
            )],
        )
        .unwrap();
        assert!(out[0].passed);
        assert!(out[0].detail.contains("line(s)"));
    }

    #[test]
    fn max_content_lines_counts_blank_separators_like_the_renderer() {
        // Two 2-line domains render as "A1\nA2\n\nB1\nB2\n" = 5 lines (blank between).
        let t = tree(
            vec![domain("a", "A1\nA2\n"), domain("b", "B1\nB2\n")],
            vec![("p", vec!["a", "b"])],
        );
        // Budget of 5 passes; 4 (the naive per-domain sum) must fail.
        let pass = evaluate(
            &t,
            &[goal(GoalKind::HardGate, "max-content-lines", Some(5), None)],
        )
        .unwrap();
        assert!(
            pass[0].passed,
            "5 lines should fit a max of 5: {}",
            pass[0].detail
        );
        let fail = evaluate(
            &t,
            &[goal(GoalKind::HardGate, "max-content-lines", Some(4), None)],
        )
        .unwrap();
        assert!(!fail[0].passed, "5 rendered lines must exceed a max of 4");
    }

    #[test]
    fn observability_failure_does_not_block() {
        let t = tree(
            vec![domain("a", "x"), domain("orphan", "y")],
            vec![("p", vec!["a"])],
        );
        let out = evaluate(
            &t,
            &[goal(GoalKind::Observability, "no-dead-domains", None, None)],
        )
        .unwrap();
        assert!(!out[0].passed);
        assert!(!out[0].is_blocking_failure(), "observability never blocks");
    }

    #[test]
    fn unknown_check_errors() {
        let t = tree(vec![domain("a", "x")], vec![("p", vec!["a"])]);
        let err = evaluate(&t, &[goal(GoalKind::HardGate, "nope", None, None)]).unwrap_err();
        assert!(matches!(err, Error::UnknownCheck { .. }));
    }
}
