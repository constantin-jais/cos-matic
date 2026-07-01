//! Candidate comparison for bounded multi-attempt autonomy.
//!
//! The scorer is intentionally conservative: a candidate must pass safety gates
//! before it can be ranked. Ranking then prefers small, green, low-risk diffs.

use crate::branch_policy::BranchPolicy;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckState {
    Pass,
    Fail,
    Pending,
    Missing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateAssessment {
    pub branch: String,
    pub tests: CheckState,
    pub lint: CheckState,
    /// Coverage delta in basis points: +100 = +1.00 percentage point.
    pub coverage_delta_bps: i32,
    pub diff_lines: u32,
    /// Paths classified as sensitive by the caller (secrets, CI policy, branch
    /// protection config, deployment credentials, etc.).
    pub sensitive_files_touched: Vec<String>,
    /// Non-PII risk notes, e.g. `large-diff`, `touches-build-system`.
    pub risk_notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CandidateVerdict {
    Eligible { score: i64 },
    Ineligible { reasons: Vec<String> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RankedCandidate {
    pub branch: String,
    pub verdict: CandidateVerdict,
}

pub fn score_candidate(policy: &BranchPolicy, c: &CandidateAssessment) -> CandidateVerdict {
    let mut reasons = Vec::new();

    if let Err(e) = policy.validate_push(&c.branch) {
        reasons.push(format!("branch-policy: {e}"));
    }
    if c.tests != CheckState::Pass {
        reasons.push(format!("tests are {:?}", c.tests));
    }
    if c.lint != CheckState::Pass {
        reasons.push(format!("lint is {:?}", c.lint));
    }
    if !c.sensitive_files_touched.is_empty() {
        reasons.push("sensitive files touched".to_string());
    }

    if !reasons.is_empty() {
        return CandidateVerdict::Ineligible { reasons };
    }

    // Baseline 10_000 keeps normal scores positive and easy to inspect.
    // Smaller diffs win; coverage and low-risk notes break ties.
    let diff_penalty = i64::from(c.diff_lines.min(5_000));
    let coverage_bonus = i64::from(c.coverage_delta_bps) / 10;
    let risk_penalty = (c.risk_notes.len() as i64) * 100;
    CandidateVerdict::Eligible {
        score: 10_000 - diff_penalty + coverage_bonus - risk_penalty,
    }
}

pub fn rank_candidates(
    policy: &BranchPolicy,
    candidates: &[CandidateAssessment],
) -> Vec<RankedCandidate> {
    let mut ranked: Vec<_> = candidates
        .iter()
        .map(|c| RankedCandidate {
            branch: c.branch.clone(),
            verdict: score_candidate(policy, c),
        })
        .collect();

    ranked.sort_by(|a, b| match (&a.verdict, &b.verdict) {
        (CandidateVerdict::Eligible { score: sa }, CandidateVerdict::Eligible { score: sb }) => {
            sb.cmp(sa).then_with(|| a.branch.cmp(&b.branch))
        }
        (CandidateVerdict::Eligible { .. }, CandidateVerdict::Ineligible { .. }) => {
            std::cmp::Ordering::Less
        }
        (CandidateVerdict::Ineligible { .. }, CandidateVerdict::Eligible { .. }) => {
            std::cmp::Ordering::Greater
        }
        (CandidateVerdict::Ineligible { .. }, CandidateVerdict::Ineligible { .. }) => {
            a.branch.cmp(&b.branch)
        }
    });

    ranked
}

pub fn select_best<'a>(
    policy: &BranchPolicy,
    candidates: &'a [CandidateAssessment],
) -> Option<&'a CandidateAssessment> {
    candidates
        .iter()
        .filter_map(|c| match score_candidate(policy, c) {
            CandidateVerdict::Eligible { score } => Some((score, c)),
            CandidateVerdict::Ineligible { .. } => None,
        })
        .max_by(|(score_a, a), (score_b, b)| {
            score_a.cmp(score_b).then_with(|| b.branch.cmp(&a.branch))
        })
        .map(|(_, c)| c)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> BranchPolicy {
        BranchPolicy::bolt_default()
    }

    fn candidate(branch: &str, diff_lines: u32) -> CandidateAssessment {
        CandidateAssessment {
            branch: branch.to_string(),
            tests: CheckState::Pass,
            lint: CheckState::Pass,
            coverage_delta_bps: 0,
            diff_lines,
            sensitive_files_touched: Vec::new(),
            risk_notes: Vec::new(),
        }
    }

    #[test]
    fn green_smaller_diff_wins() {
        let large = candidate("bolt/run/r/issue-1/attempt-1", 500);
        let small = candidate("bolt/run/r/issue-1/attempt-2", 20);
        let candidates = [large, small];
        let best = select_best(&policy(), &candidates).unwrap();
        assert_eq!(best.branch, "bolt/run/r/issue-1/attempt-2");
    }

    #[test]
    fn coverage_can_break_close_tie() {
        let mut lower = candidate("bolt/run/r/issue-1/attempt-1", 100);
        lower.coverage_delta_bps = 0;
        let mut higher = candidate("bolt/run/r/issue-1/attempt-2", 105);
        higher.coverage_delta_bps = 100;
        let candidates = [lower, higher];
        let best = select_best(&policy(), &candidates).unwrap();
        assert_eq!(best.branch, "bolt/run/r/issue-1/attempt-2");
    }

    #[test]
    fn failing_tests_are_ineligible() {
        let mut c = candidate("bolt/run/r/issue-1/attempt-1", 1);
        c.tests = CheckState::Fail;
        match score_candidate(&policy(), &c) {
            CandidateVerdict::Ineligible { reasons } => {
                assert!(reasons.iter().any(|r| r.contains("tests")))
            }
            other => panic!("expected ineligible, got {other:?}"),
        }
    }

    #[test]
    fn pending_lint_is_ineligible() {
        let mut c = candidate("bolt/run/r/issue-1/attempt-1", 1);
        c.lint = CheckState::Pending;
        assert!(matches!(
            score_candidate(&policy(), &c),
            CandidateVerdict::Ineligible { .. }
        ));
    }

    #[test]
    fn sensitive_file_touch_is_ineligible() {
        let mut c = candidate("bolt/run/r/issue-1/attempt-1", 1);
        c.sensitive_files_touched
            .push(".github/workflows/ci.yml".into());
        match score_candidate(&policy(), &c) {
            CandidateVerdict::Ineligible { reasons } => {
                assert!(reasons.iter().any(|r| r.contains("sensitive")))
            }
            other => panic!("expected ineligible, got {other:?}"),
        }
    }

    #[test]
    fn branch_outside_owned_namespace_is_ineligible() {
        let c = candidate("feature/human", 1);
        match score_candidate(&policy(), &c) {
            CandidateVerdict::Ineligible { reasons } => {
                assert!(reasons.iter().any(|r| r.contains("branch-policy")))
            }
            other => panic!("expected ineligible, got {other:?}"),
        }
    }

    #[test]
    fn ranking_puts_eligible_before_ineligible() {
        let mut bad = candidate("bolt/run/r/issue-1/attempt-1", 1);
        bad.tests = CheckState::Fail;
        let good = candidate("bolt/run/r/issue-1/attempt-2", 1000);
        let ranked = rank_candidates(&policy(), &[bad, good]);
        assert_eq!(ranked[0].branch, "bolt/run/r/issue-1/attempt-2");
        assert!(matches!(
            ranked[0].verdict,
            CandidateVerdict::Eligible { .. }
        ));
    }
}
