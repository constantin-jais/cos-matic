//! Autonomous merge: the gate-and-merge step. The cardinal rule — **nothing
//! merges without attached green evidence** — plus the binding envelope
//! (kill-switch, scope-fence, rate-limit) and a zero-PII audit. The gate verdict
//! and the merge are traits, so the whole decision is proven offline; the real
//! CI check and merge (via `gh`) are the live boundary. This is the action that
//! lands code, so ADR: workspace-and-orchestrator-charter is fully binding here.

use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::forge::RepoId;

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

/// A gate or merge failure (spawn, API, parse).
#[derive(Debug)]
pub struct MergeError(pub String);

impl std::fmt::Display for MergeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "automerge error: {}", self.0)
    }
}

impl std::error::Error for MergeError {}

/// Produces the evidence verdict for a branch (CI status / a local gate run).
pub trait Gate {
    fn verdict(&self, req: &MergeRequest) -> Result<Verdict, MergeError>;
}

/// Performs the actual merge once the gate is green. Real impl = `gh pr merge`.
pub trait Merger {
    /// Merge the branch and return a reference (URL / sha) to the merge.
    fn merge(&self, req: &MergeRequest) -> Result<String, MergeError>;
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
pub fn auto_merge<G: Gate + ?Sized, M: Merger + ?Sized>(
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
    match gate.verdict(req)? {
        Verdict::Green => {
            let reference = merger.merge(req)?;
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
    /// No readable checks yet: either none have registered after a fresh push, or
    /// `gh` reports "no checks" before the first one appears. In the loop the PR
    /// always exists, so this means "not yet", not "never" — worth waiting.
    NoChecks,
}

/// Classify a set of `(name, bucket)` checks into a poll state. Pure, so the
/// branching (which `gh` buckets mean pass/fail/wait) is unit-tested; only the
/// `gh` call and the wait around it are the live boundary.
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

/// Real gate: the branch's PR checks via `gh`, **waited until they settle**. A
/// merge gate that read a freshly-pushed PR once and called its pending checks
/// `Unknown` would block every real run; instead it polls (every `interval`, up
/// to `timeout`) until the checks pass or fail. All passing -> Green; any failing
/// -> Red; no PR / no checks / still pending at the deadline -> Unknown
/// (fail-closed). The poll loop (subprocess + sleep) is the live boundary; the
/// classification is unit-tested.
pub struct GhChecksGate {
    pub timeout: Duration,
    pub interval: Duration,
}

impl Default for GhChecksGate {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(600),
            interval: Duration::from_secs(15),
        }
    }
}

impl GhChecksGate {
    fn poll_once(&self, req: &MergeRequest) -> Result<PollState, MergeError> {
        let mut cmd = Command::new("gh");
        cmd.args(["pr", "checks", &req.branch, "--json", "name,bucket"]);
        // Reading checks needs a different scope than writing them: let the
        // caller hand the gate a dedicated read-only token (e.g. a CI runner's
        // github.token) via AOM_CHECKS_TOKEN, so the write token (which pushes,
        // opens, and merges the PR) need not also carry a Checks scope. `gh`
        // prefers GH_TOKEN over GITHUB_TOKEN, so this only affects this call.
        if let Ok(tok) = std::env::var("AOM_CHECKS_TOKEN")
            && !tok.is_empty()
        {
            cmd.env("GH_TOKEN", tok);
        }
        let out = cmd
            .output()
            .map_err(|e| MergeError(format!("spawn gh: {e}")))?;
        // `gh pr checks` exits non-zero when checks are merely failing (1) or
        // pending (8) — that is status, not error — so classify from the JSON,
        // never the exit code. A non-array body means no checks are readable yet
        // (none registered, or "no checks reported" right after a push): wait.
        let Ok(serde_json::Value::Array(arr)) =
            serde_json::from_slice::<serde_json::Value>(&out.stdout)
        else {
            return Ok(PollState::NoChecks);
        };
        let checks: Vec<(String, String)> = arr
            .iter()
            .map(|c| {
                (
                    c["name"].as_str().unwrap_or("?").to_string(),
                    c["bucket"].as_str().unwrap_or("").to_string(),
                )
            })
            .collect();
        Ok(classify(&checks))
    }
}

impl Gate for GhChecksGate {
    fn verdict(&self, req: &MergeRequest) -> Result<Verdict, MergeError> {
        let deadline = Instant::now() + self.timeout;
        loop {
            match self.poll_once(req)? {
                PollState::Settled(v) => return Ok(v),
                // Pending, or checks not registered yet: wait for them to settle,
                // bounded by the timeout (then fail-closed to Unknown).
                PollState::NoChecks | PollState::Pending => {
                    if Instant::now() >= deadline {
                        return Ok(Verdict::Unknown);
                    }
                    std::thread::sleep(self.interval);
                }
            }
        }
    }
}

/// Real merger: `gh pr merge --rebase` for the branch. Reversible by revert.
pub struct GhMerger;

impl Merger for GhMerger {
    fn merge(&self, req: &MergeRequest) -> Result<String, MergeError> {
        let out = Command::new("gh")
            .args(["pr", "merge", &req.branch, "--rebase"])
            .output()
            .map_err(|e| MergeError(format!("spawn gh: {e}")))?;
        if !out.status.success() {
            return Err(MergeError(format!(
                "gh pr merge failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            )));
        }
        Ok(format!("merged {}", req.branch))
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    struct FakeGate(Verdict);
    impl Gate for FakeGate {
        fn verdict(&self, _req: &MergeRequest) -> Result<Verdict, MergeError> {
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
    impl Merger for FakeMerger {
        fn merge(&self, _req: &MergeRequest) -> Result<String, MergeError> {
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
            branch: "aom/fix/issue-8".into(),
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

    #[test]
    fn green_merges() {
        let m = FakeMerger::new();
        let r = auto_merge(
            &FakeGate(Verdict::Green),
            &m,
            &env(true, vec![repo()], 1),
            &req(),
            0,
        )
        .unwrap();
        assert!(matches!(r, MergeOutcome::Merged { .. }));
        assert!(m.merged.get(), "a green branch is merged");
    }

    #[test]
    fn red_never_merges() {
        let m = FakeMerger::new();
        let gate = FakeGate(Verdict::Red {
            reasons: vec!["tests".into()],
        });
        let r = auto_merge(&gate, &m, &env(true, vec![repo()], 1), &req(), 0).unwrap();
        assert!(matches!(r, MergeOutcome::Refused { .. }));
        assert!(!m.merged.get(), "CARDINAL: a red branch is NEVER merged");
    }

    #[test]
    fn unknown_fails_closed() {
        let m = FakeMerger::new();
        let r = auto_merge(
            &FakeGate(Verdict::Unknown),
            &m,
            &env(true, vec![repo()], 1),
            &req(),
            0,
        )
        .unwrap();
        assert!(matches!(r, MergeOutcome::Refused { .. }));
        assert!(
            !m.merged.get(),
            "an unprovable verdict never merges (fail-closed)"
        );
    }

    #[test]
    fn kill_switch_refuses_before_gating() {
        let m = FakeMerger::new();
        let r = auto_merge(
            &FakeGate(Verdict::Green),
            &m,
            &env(false, vec![repo()], 1),
            &req(),
            0,
        )
        .unwrap();
        assert!(matches!(r, MergeOutcome::Refused { .. }));
        assert!(!m.merged.get());
    }

    #[test]
    fn scope_fence_refuses_off_allowlist() {
        let m = FakeMerger::new();
        let r = auto_merge(
            &FakeGate(Verdict::Green),
            &m,
            &env(true, vec![], 1),
            &req(),
            0,
        )
        .unwrap();
        match r {
            MergeOutcome::Refused { reason } => assert!(reason.contains("scope-fence")),
            other => panic!("expected scope-fence refusal, got {other:?}"),
        }
        assert!(!m.merged.get());
    }

    #[test]
    fn rate_limit_refuses_when_exhausted() {
        let m = FakeMerger::new();
        let r = auto_merge(
            &FakeGate(Verdict::Green),
            &m,
            &env(true, vec![repo()], 1),
            &req(),
            1,
        )
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
