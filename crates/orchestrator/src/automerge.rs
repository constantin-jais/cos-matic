//! Autonomous merge: the gate-and-merge step. The cardinal rule — **nothing
//! merges without attached green evidence** — plus the binding envelope
//! (kill-switch, scope-fence, rate-limit) and a zero-PII audit. The gate verdict
//! and the merge are traits, so the whole decision is proven offline; the real
//! CI check and merge (via the `Forge`/octocrab) are the live boundary. This is
//! the action that lands code, so ADR: workspace-and-orchestrator-charter is
//! fully binding here.

use std::path::Path;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde::Serialize;

use crate::forge::{Forge, MergeStrategy, RepoId};

/// A branch proposed for an autonomous merge.
#[derive(Debug, Clone)]
pub struct MergeRequest {
    pub branch: String,
    pub repo: RepoId,
}

/// The evidence verdict for a branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Attached green proof — every required check passed.
    Green,
    /// Red — at least one required check failed.
    Red { reasons: Vec<String> },
    /// Verdict unavailable (checks pending or missing). Fail-closed: never merge.
    Unknown,
}

/// A gate or merge failure (network, API).
#[derive(Debug)]
pub struct MergeError(pub String);

impl std::fmt::Display for MergeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "automerge error: {}", self.0)
    }
}

impl std::error::Error for MergeError {}

/// Produces the evidence verdict for a branch (CI status / a local gate run).
#[async_trait(?Send)]
pub trait Gate {
    async fn verdict(&self, req: &MergeRequest) -> Result<Verdict, MergeError>;
}

/// Performs the actual merge once the gate is green. Real impl = forge merge.
#[async_trait(?Send)]
pub trait Merger {
    /// Merge the branch and return a reference (URL / sha) to the merge.
    async fn merge(&self, req: &MergeRequest) -> Result<String, MergeError>;
}

/// The binding envelope for an autonomous merge (ADR: workspace-and-orchestrator-charter).
#[derive(Debug, Clone)]
pub struct MergeEnvelope {
    /// Kill-switch: when false, every merge is refused.
    pub enabled: bool,
    /// Scope-fence: only repos on this allowlist may be merged.
    pub allowlist: Vec<RepoId>,
    /// Rate-limit / circuit-breaker: max autonomous merges per run.
    pub max_merges: u32,
}

/// The result: refused by the envelope or a red/unknown gate, or merged with
/// attached green evidence.
#[derive(Debug)]
pub enum MergeOutcome {
    Refused { reason: String },
    Merged { reference: String },
}

impl MergeOutcome {
    fn outcome(&self) -> &'static str {
        match self {
            MergeOutcome::Refused { .. } => "refused",
            MergeOutcome::Merged { .. } => "merged",
        }
    }
}

/// Gate-and-merge inside the envelope. The merger is invoked **only** on a Green
/// verdict; Red and Unknown both refuse (fail-closed). Envelope violations refuse
/// without ever consulting the gate or merger.
pub async fn auto_merge<G: Gate + ?Sized, M: Merger + ?Sized>(
    gate: &G,
    merger: &M,
    env: &MergeEnvelope,
    req: &MergeRequest,
    merges_this_run: u32,
) -> Result<MergeOutcome, MergeError> {
    if !env.enabled {
        return Ok(MergeOutcome::Refused {
            reason: "auto-merge is disabled (kill-switch)".to_string(),
        });
    }
    if !env.allowlist.contains(&req.repo) {
        return Ok(MergeOutcome::Refused {
            reason: format!(
                "scope-fence: {}/{} is not on the auto-merge allowlist",
                req.repo.owner, req.repo.name
            ),
        });
    }
    if merges_this_run >= env.max_merges {
        return Ok(MergeOutcome::Refused {
            reason: format!(
                "rate-limit: {merges_this_run} merge(s) already this run (max {})",
                env.max_merges
            ),
        });
    }
    // CARDINAL RULE: nothing merges without attached green evidence.
    match gate.verdict(req).await? {
        Verdict::Green => {
            let reference = merger.merge(req).await?;
            Ok(MergeOutcome::Merged { reference })
        }
        Verdict::Red { reasons } => Ok(MergeOutcome::Refused {
            reason: format!("gate is red: {}", reasons.join("; ")),
        }),
        Verdict::Unknown => Ok(MergeOutcome::Refused {
            reason: "gate verdict unavailable — fail-closed, not merging".to_string(),
        }),
    }
}

/// A zero-PII audit record of an auto-merge decision.
#[derive(Serialize)]
struct AuditEntry<'a> {
    action: &'a str,
    repo: String,
    branch: String,
    outcome: &'a str,
    ts: u64,
}

/// Append a zero-PII audit line for an auto-merge decision — every autonomous
/// action is recorded (ADR: workspace-and-orchestrator-charter).
pub fn append_audit(
    dir: &Path,
    req: &MergeRequest,
    outcome: &MergeOutcome,
    ts: u64,
) -> std::io::Result<()> {
    use std::io::Write as _;
    std::fs::create_dir_all(dir)?;
    let entry = AuditEntry {
        action: "automerge",
        repo: format!("{}/{}", req.repo.owner, req.repo.name),
        branch: req.branch.clone(),
        outcome: outcome.outcome(),
        ts,
    };
    let line = serde_json::to_string(&entry).map_err(std::io::Error::other)?;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("automerge.jsonl"))?;
    writeln!(f, "{line}")
}

/// What a single poll of the PR checks shows.
#[derive(Debug, PartialEq, Eq)]
enum PollState {
    /// Checks have settled to a final verdict.
    Settled(Verdict),
    /// At least one check is still running — worth waiting for.
    Pending,
    /// No readable checks yet: none have registered after a fresh push. In the
    /// loop the PR always exists, so this means "not yet", not "never" — worth
    /// waiting.
    NoChecks,
}

/// Classify a set of `(name, bucket)` checks into a poll state. Pure, so the
/// branching (which buckets mean pass/fail/wait) is unit-tested; only the forge
/// call and the wait around it are the live boundary.
fn classify(checks: &[(String, String)]) -> PollState {
    if checks.is_empty() {
        return PollState::NoChecks;
    }
    let mut failing = Vec::new();
    let mut pending = false;
    for (name, bucket) in checks {
        match bucket.as_str() {
            // `skipping` is a final, non-blocking state — not something to wait on.
            "pass" | "skipping" => {}
            "fail" | "cancel" => failing.push(name.clone()),
            _ => pending = true,
        }
    }
    if !failing.is_empty() {
        PollState::Settled(Verdict::Red { reasons: failing })
    } else if pending {
        PollState::Pending
    } else {
        PollState::Settled(Verdict::Green)
    }
}

/// Real gate: the branch's PR checks via the `Forge`, **waited until they
/// settle**. A merge gate that read a freshly-pushed PR once and called its
/// pending checks `Unknown` would block every real run; instead it polls (every
/// `interval`, up to `timeout`) until the checks pass or fail. All passing ->
/// Green; any failing -> Red; no PR / still pending at the deadline -> Unknown
/// (fail-closed). The poll loop (forge call + sleep) is the live boundary; the
/// classification is unit-tested.
pub struct ForgeGate<'a, F: Forge + ?Sized> {
    pub forge: &'a F,
    pub timeout: Duration,
    pub interval: Duration,
}

impl<'a, F: Forge + ?Sized> ForgeGate<'a, F> {
    /// Default cadence: poll every 10s, up to 3 minutes.
    pub fn new(forge: &'a F) -> Self {
        Self {
            forge,
            timeout: Duration::from_secs(180),
            interval: Duration::from_secs(10),
        }
    }
}

#[async_trait(?Send)]
impl<F: Forge + Sync + ?Sized> Gate for ForgeGate<'_, F> {
    async fn verdict(&self, req: &MergeRequest) -> Result<Verdict, MergeError> {
        // No open PR for the branch -> nothing to gate; fail closed at once
        // rather than polling to the timeout. Only reachable from a standalone
        // automerge; the loop always publishes a PR first.
        if self
            .forge
            .find_open_pr(&req.repo, &req.branch)
            .await
            .map_err(|e| MergeError(e.0))?
            .is_none()
        {
            eprintln!("[gate] {}: no open PR -> Unknown", req.branch);
            return Ok(Verdict::Unknown);
        }
        let deadline = Instant::now() + self.timeout;
        let verdict = loop {
            let checks = self
                .forge
                .list_check_runs(&req.repo, &req.branch)
                .await
                .map_err(|e| MergeError(e.0))?;
            match classify(&checks) {
                PollState::Settled(v) => break v,
                // Pending, or checks not registered yet: wait for them to settle,
                // bounded by the timeout (then fail-closed to Unknown).
                PollState::NoChecks | PollState::Pending => {
                    if Instant::now() >= deadline {
                        break Verdict::Unknown;
                    }
                    tokio::time::sleep(self.interval).await;
                }
            }
        };
        // One line per gate decision (not per poll) — enough to follow an
        // autonomous run without flooding the log.
        eprintln!("[gate] {}: {verdict:?}", req.branch);
        Ok(verdict)
    }
}

/// Real merger: a rebase merge of the branch's PR via the `Forge`. Reversible by
/// revert. octocrab merges by PR number, so it looks the PR up first.
pub struct ForgeMerger<'a, F: Forge + ?Sized> {
    pub forge: &'a F,
}

impl<'a, F: Forge + ?Sized> ForgeMerger<'a, F> {
    pub fn new(forge: &'a F) -> Self {
        Self { forge }
    }
}

#[async_trait(?Send)]
impl<F: Forge + Sync + ?Sized> Merger for ForgeMerger<'_, F> {
    async fn merge(&self, req: &MergeRequest) -> Result<String, MergeError> {
        let pr = self
            .forge
            .find_open_pr(&req.repo, &req.branch)
            .await
            .map_err(|e| MergeError(e.0))?
            .ok_or_else(|| MergeError(format!("no open PR for {}", req.branch)))?;
        let reference = self
            .forge
            .merge_pr(&req.repo, pr.number, MergeStrategy::Rebase)
            .await
            .map_err(|e| MergeError(e.0))?;
        eprintln!("[merge] {} merged", req.branch);
        Ok(reference)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;
    use crate::forge::FakeForge;

    struct FakeGate(Verdict);
    #[async_trait(?Send)]
    impl Gate for FakeGate {
        async fn verdict(&self, _req: &MergeRequest) -> Result<Verdict, MergeError> {
            Ok(self.0.clone())
        }
    }

    /// Records whether `merge` was called — the cardinal-rule assertion.
    struct FakeMerger {
        merged: Cell<bool>,
    }
    impl FakeMerger {
        fn new() -> Self {
            Self {
                merged: Cell::new(false),
            }
        }
    }
    #[async_trait(?Send)]
    impl Merger for FakeMerger {
        async fn merge(&self, _req: &MergeRequest) -> Result<String, MergeError> {
            self.merged.set(true);
            Ok("fake-merge".to_string())
        }
    }

    fn repo() -> RepoId {
        RepoId {
            owner: "o".into(),
            name: "n".into(),
        }
    }
    fn req() -> MergeRequest {
        MergeRequest {
            branch: "bolt/fix/issue-8".into(),
            repo: repo(),
        }
    }
    fn env(enabled: bool, allow: Vec<RepoId>, max: u32) -> MergeEnvelope {
        MergeEnvelope {
            enabled,
            allowlist: allow,
            max_merges: max,
        }
    }

    #[tokio::test]
    async fn green_merges() {
        let m = FakeMerger::new();
        let r = auto_merge(
            &FakeGate(Verdict::Green),
            &m,
            &env(true, vec![repo()], 1),
            &req(),
            0,
        )
        .await
        .unwrap();
        assert!(matches!(r, MergeOutcome::Merged { .. }));
        assert!(m.merged.get(), "a green branch is merged");
    }

    #[tokio::test]
    async fn red_never_merges() {
        let m = FakeMerger::new();
        let gate = FakeGate(Verdict::Red {
            reasons: vec!["tests".into()],
        });
        let r = auto_merge(&gate, &m, &env(true, vec![repo()], 1), &req(), 0)
            .await
            .unwrap();
        assert!(matches!(r, MergeOutcome::Refused { .. }));
        assert!(!m.merged.get(), "CARDINAL: a red branch is NEVER merged");
    }

    #[tokio::test]
    async fn unknown_fails_closed() {
        let m = FakeMerger::new();
        let r = auto_merge(
            &FakeGate(Verdict::Unknown),
            &m,
            &env(true, vec![repo()], 1),
            &req(),
            0,
        )
        .await
        .unwrap();
        assert!(matches!(r, MergeOutcome::Refused { .. }));
        assert!(
            !m.merged.get(),
            "an unprovable verdict never merges (fail-closed)"
        );
    }

    #[tokio::test]
    async fn kill_switch_refuses_before_gating() {
        let m = FakeMerger::new();
        let r = auto_merge(
            &FakeGate(Verdict::Green),
            &m,
            &env(false, vec![repo()], 1),
            &req(),
            0,
        )
        .await
        .unwrap();
        assert!(matches!(r, MergeOutcome::Refused { .. }));
        assert!(!m.merged.get());
    }

    #[tokio::test]
    async fn scope_fence_refuses_off_allowlist() {
        let m = FakeMerger::new();
        let r = auto_merge(
            &FakeGate(Verdict::Green),
            &m,
            &env(true, vec![], 1),
            &req(),
            0,
        )
        .await
        .unwrap();
        match r {
            MergeOutcome::Refused { reason } => assert!(reason.contains("scope-fence")),
            other => panic!("expected scope-fence refusal, got {other:?}"),
        }
        assert!(!m.merged.get());
    }

    #[tokio::test]
    async fn rate_limit_refuses_when_exhausted() {
        let m = FakeMerger::new();
        let r = auto_merge(
            &FakeGate(Verdict::Green),
            &m,
            &env(true, vec![repo()], 1),
            &req(),
            1,
        )
        .await
        .unwrap();
        match r {
            MergeOutcome::Refused { reason } => assert!(reason.contains("rate-limit")),
            other => panic!("expected rate-limit refusal, got {other:?}"),
        }
        assert!(!m.merged.get());
    }

    #[test]
    fn audit_line_is_zero_pii() {
        let dir = tempfile::tempdir().unwrap();
        let outcome = MergeOutcome::Merged {
            reference: "r".into(),
        };
        append_audit(dir.path(), &req(), &outcome, 100).unwrap();
        let body = std::fs::read_to_string(dir.path().join("automerge.jsonl")).unwrap();
        let v: serde_json::Value = serde_json::from_str(body.trim()).unwrap();
        assert_eq!(v["action"], "automerge");
        assert_eq!(v["repo"], "o/n");
        assert_eq!(v["outcome"], "merged");
        assert_eq!(v.as_object().unwrap().len(), 5);
    }

    // --- the forge-backed gate/merger, exercised against the in-memory forge ---

    fn fast_gate<F: Forge + ?Sized>(forge: &F) -> ForgeGate<'_, F> {
        ForgeGate {
            forge,
            timeout: Duration::from_millis(50),
            interval: Duration::from_millis(5),
        }
    }

    #[tokio::test]
    async fn forge_gate_no_pr_is_unknown() {
        let forge = FakeForge::new();
        assert_eq!(
            fast_gate(&forge).verdict(&req()).await.unwrap(),
            Verdict::Unknown
        );
    }

    #[tokio::test]
    async fn forge_gate_all_pass_is_green() {
        let forge = FakeForge::new()
            .with_pr("bolt/fix/issue-8", 1)
            .with_checks(&[("ci", "pass"), ("lint", "pass")]);
        assert_eq!(
            fast_gate(&forge).verdict(&req()).await.unwrap(),
            Verdict::Green
        );
    }

    #[tokio::test]
    async fn forge_gate_a_fail_is_red() {
        let forge = FakeForge::new()
            .with_pr("bolt/fix/issue-8", 1)
            .with_checks(&[("ci", "pass"), ("lint", "fail")]);
        assert!(matches!(
            fast_gate(&forge).verdict(&req()).await.unwrap(),
            Verdict::Red { .. }
        ));
    }

    #[tokio::test]
    async fn forge_merger_merges_the_open_pr() {
        let forge = FakeForge::new().with_pr("bolt/fix/issue-8", 7);
        let r = ForgeMerger::new(&forge).merge(&req()).await.unwrap();
        assert!(r.contains("#7"), "merges the looked-up PR number");
    }

    fn checks(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(n, b)| (n.to_string(), b.to_string()))
            .collect()
    }

    #[test]
    fn classify_empty_is_no_checks() {
        assert_eq!(classify(&[]), PollState::NoChecks);
    }

    #[test]
    fn classify_all_pass_is_green() {
        assert_eq!(
            classify(&checks(&[("ci", "pass"), ("lint", "pass")])),
            PollState::Settled(Verdict::Green)
        );
    }

    #[test]
    fn classify_any_fail_is_red() {
        match classify(&checks(&[("ci", "pass"), ("lint", "fail")])) {
            PollState::Settled(Verdict::Red { reasons }) => assert_eq!(reasons, ["lint"]),
            other => panic!("expected Red, got {other:?}"),
        }
    }

    #[test]
    fn classify_pending_waits() {
        assert_eq!(
            classify(&checks(&[("ci", "pass"), ("slow", "pending")])),
            PollState::Pending
        );
    }

    #[test]
    fn classify_skipping_does_not_block() {
        assert_eq!(
            classify(&checks(&[("ci", "pass"), ("opt", "skipping")])),
            PollState::Settled(Verdict::Green)
        );
    }
}
