//! Autonomous merge: the gate-and-merge step. The cardinal rule — **nothing
//! merges without attached green evidence** — plus the binding envelope
//! (kill-switch, scope-fence, rate-limit) and a zero-PII audit. The gate verdict
//! and the merge are traits, so the whole decision is proven offline; the real
//! CI check and merge (via `gh`) are the live boundary. This is the action that
//! lands code, so ADR: workspace-and-orchestrator-charter is fully binding here.

use std::path::Path;
use std::process::Command;

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

/// Real gate: the branch's PR checks via `gh`. All passing -> Green; any failing
/// -> Red; anything pending/missing -> Unknown (fail-closed). Subprocess, so it
/// is exercised live, not in unit tests.
pub struct GhChecksGate;

impl Gate for GhChecksGate {
    fn verdict(&self, req: &MergeRequest) -> Result<Verdict, MergeError> {
        let out = Command::new("gh")
            .args(["pr", "checks", &req.branch, "--json", "name,bucket"])
            .output()
            .map_err(|e| MergeError(format!("spawn gh: {e}")))?;
        if !out.status.success() {
            // No PR / no checks yet -> we cannot prove green. Fail-closed.
            return Ok(Verdict::Unknown);
        }
        let v: serde_json::Value = serde_json::from_slice(&out.stdout)
            .map_err(|e| MergeError(format!("parse gh output: {e}")))?;
        let checks = v.as_array().cloned().unwrap_or_default();
        if checks.is_empty() {
            return Ok(Verdict::Unknown);
        }
        let mut failing = Vec::new();
        let mut pending = false;
        for c in &checks {
            match c["bucket"].as_str() {
                Some("pass") => {}
                Some("fail") | Some("cancel") => {
                    failing.push(c["name"].as_str().unwrap_or("?").to_string());
                }
                _ => pending = true,
            }
        }
        if !failing.is_empty() {
            Ok(Verdict::Red { reasons: failing })
        } else if pending {
            Ok(Verdict::Unknown)
        } else {
            Ok(Verdict::Green)
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
}
