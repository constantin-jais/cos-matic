//! The end-to-end loop: dispatch -> publish -> automerge -> deploy, under one
//! global envelope (kill-switch, scope-fence, circuit-breaker) that
//! **short-circuits at the first stage that does not advance**. The stages are
//! abstracted behind a `Stages` trait, so the composition (ordering,
//! short-circuit, envelope) is proven offline; `RealStages` wires the real
//! primitives, the live boundary. ADR: workspace-and-orchestrator-charter
//! governs the whole chain.

use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

use serde::Serialize;

use crate::forge::RepoId;
use crate::{automerge, deploy, dispatch};

/// The incident driving one loop iteration.
#[derive(Debug, Clone)]
pub struct LoopRequest {
    pub issue: u64,
    pub title: String,
    pub body: String,
    pub repo: RepoId,
}

/// A loop-stage failure.
#[derive(Debug)]
pub struct LoopError(pub String);

impl std::fmt::Display for LoopError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "loop error: {}", self.0)
    }
}

impl std::error::Error for LoopError {}

/// The loop's stages; each reports whether it advanced.
pub trait Stages {
    /// Dispatch a bounded fix; returns the produced branch, or None if none.
    fn dispatch(&self, req: &LoopRequest) -> Result<Option<String>, LoopError>;
    /// Publish the branch (push + open a PR) so the gate has something to check;
    /// true if a PR is now open. Dispatch deliberately does not push — this is
    /// the distinct, opt-in step that does.
    fn publish(&self, branch: &str, req: &LoopRequest) -> Result<bool, LoopError>;
    /// Gate-and-merge the branch on green evidence; true if merged.
    fn automerge(&self, branch: &str, req: &LoopRequest) -> Result<bool, LoopError>;
    /// Canary-deploy + smoke the merged result; true if promoted.
    fn deploy(&self, branch: &str, req: &LoopRequest) -> Result<bool, LoopError>;
}

/// The global envelope for the whole loop, on top of each stage's own.
#[derive(Debug, Clone)]
pub struct LoopEnvelope {
    /// Global kill-switch.
    pub enabled: bool,
    /// Scope-fence: only repos on this allowlist may run the loop.
    pub allowlist: Vec<RepoId>,
    /// Global circuit-breaker: max loop iterations.
    pub max_iterations: u32,
}

#[derive(Debug, PartialEq, Eq)]
pub enum LoopOutcome {
    /// The global envelope refused the loop.
    Refused { reason: String },
    /// A stage did not advance; later stages were never reached.
    Stopped { stage: &'static str, reason: String },
    /// dispatched -> published -> merged -> deployed.
    Completed { branch: String },
}

impl LoopOutcome {
    fn outcome(&self) -> &'static str {
        match self {
            LoopOutcome::Refused { .. } => "refused",
            LoopOutcome::Stopped { .. } => "stopped",
            LoopOutcome::Completed { .. } => "completed",
        }
    }
}

/// Run the loop once. It stops at the first stage that does not advance — a later
/// stage is **never reached** after an earlier one stops, so the loop is fail-safe
/// by construction. Envelope refusals never touch a stage.
pub fn run_loop<S: Stages + ?Sized>(
    stages: &S,
    env: &LoopEnvelope,
    req: &LoopRequest,
    iterations_so_far: u32,
) -> Result<LoopOutcome, LoopError> {
    if !env.enabled {
        return Ok(LoopOutcome::Refused {
            reason: "loop is disabled (kill-switch)".to_string(),
        });
    }
    if !env.allowlist.contains(&req.repo) {
        return Ok(LoopOutcome::Refused {
            reason: format!(
                "scope-fence: {}/{} is not on the loop allowlist",
                req.repo.owner, req.repo.name
            ),
        });
    }
    if iterations_so_far >= env.max_iterations {
        return Ok(LoopOutcome::Refused {
            reason: format!(
                "circuit-breaker: {iterations_so_far} iteration(s) already (max {})",
                env.max_iterations
            ),
        });
    }

    let branch = match stages.dispatch(req)? {
        Some(b) => b,
        None => {
            return Ok(LoopOutcome::Stopped {
                stage: "dispatch",
                reason: "no fix branch was produced".to_string(),
            });
        }
    };
    if !stages.publish(&branch, req)? {
        return Ok(LoopOutcome::Stopped {
            stage: "publish",
            reason: "branch was not published (push or PR failed)".to_string(),
        });
    }
    if !stages.automerge(&branch, req)? {
        return Ok(LoopOutcome::Stopped {
            stage: "automerge",
            reason: "branch was not merged (gate not green)".to_string(),
        });
    }
    if !stages.deploy(&branch, req)? {
        return Ok(LoopOutcome::Stopped {
            stage: "deploy",
            reason: "deploy rolled back (smoke not green)".to_string(),
        });
    }
    Ok(LoopOutcome::Completed { branch })
}

/// Run the loop, retrying each time a stage stops, until it completes or the
/// circuit-breaker is exhausted. A stop means the attempt did not land (no fix
/// produced, a red gate, a rolled-back deploy) — so the loop tries again, bounded
/// by `env.max_iterations`. Envelope refusals (kill-switch, scope-fence) are
/// terminal and never retried. On exhaustion the **last stop** is returned (its
/// stage and reason), not a bare circuit-breaker notice, so the caller learns why
/// the loop never landed.
pub fn run_until_done<S: Stages + ?Sized>(
    stages: &S,
    env: &LoopEnvelope,
    req: &LoopRequest,
) -> Result<LoopOutcome, LoopError> {
    let mut iteration = 0;
    let mut last_stop: Option<LoopOutcome> = None;
    loop {
        match run_loop(stages, env, req, iteration)? {
            done @ LoopOutcome::Completed { .. } => return Ok(done),
            LoopOutcome::Refused { reason } => {
                // Circuit-breaker exhaustion after one or more stops returns the
                // last stop (more useful than "max iterations"); a first-pass
                // refusal (kill-switch / scope-fence) has no prior stop and is
                // returned as-is.
                return Ok(last_stop.unwrap_or(LoopOutcome::Refused { reason }));
            }
            stop @ LoopOutcome::Stopped { .. } => {
                last_stop = Some(stop);
                iteration += 1;
            }
        }
    }
}

/// A zero-PII audit record of a loop run.
#[derive(Serialize)]
struct AuditEntry<'a> {
    action: &'a str,
    issue: u64,
    repo: String,
    outcome: &'a str,
    ts: u64,
}

/// Append a zero-PII audit line for a loop run (ADR: workspace-and-orchestrator-charter).
pub fn append_audit(
    dir: &Path,
    req: &LoopRequest,
    outcome: &LoopOutcome,
    ts: u64,
) -> std::io::Result<()> {
    use std::io::Write as _;
    std::fs::create_dir_all(dir)?;
    let entry = AuditEntry {
        action: "loop",
        issue: req.issue,
        repo: format!("{}/{}", req.repo.owner, req.repo.name),
        outcome: outcome.outcome(),
        ts,
    };
    let line = serde_json::to_string(&entry).map_err(std::io::Error::other)?;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("loop.jsonl"))?;
    writeln!(f, "{line}")
}

/// The real stages: each wires the corresponding primitive, scope-fenced to the
/// loop's repo. Subprocess + network throughout, so it is exercised live, not in
/// unit tests. The deploy commands come from the environment, like `aom deploy`.
pub struct RealStages {
    pub repo_root: PathBuf,
    pub deploy_canary: String,
    pub deploy_promote: String,
    pub deploy_rollback: String,
    pub deploy_smoke: String,
}

fn one(repo: &RepoId) -> Vec<RepoId> {
    vec![repo.clone()]
}

impl Stages for RealStages {
    fn dispatch(&self, req: &LoopRequest) -> Result<Option<String>, LoopError> {
        let env = dispatch::Envelope {
            enabled: true,
            allowlist: one(&req.repo),
            max_attempts: 1,
        };
        let fix = dispatch::FixRequest {
            issue: req.issue,
            title: req.title.clone(),
            body: req.body.clone(),
            repo: req.repo.clone(),
        };
        // AOM_FIXER=stub selects the deterministic no-LLM fixer (validate the loop
        // plumbing without an Anthropic key); anything else uses the real Claude.
        let root = self.repo_root.clone();
        let report = if std::env::var("AOM_FIXER").as_deref() == Ok("stub") {
            dispatch::dispatch(&dispatch::StubFixer { repo_root: root }, &env, &fix)
        } else {
            dispatch::dispatch(&dispatch::ClaudeFixer { repo_root: root }, &env, &fix)
        }
        .map_err(|e| LoopError(e.0))?;
        Ok(match report {
            dispatch::DispatchReport::Attempted { branch, .. } => Some(branch),
            dispatch::DispatchReport::Refused { .. } => None,
        })
    }

    fn publish(&self, branch: &str, req: &LoopRequest) -> Result<bool, LoopError> {
        // Push the local fix branch, then open a PR so the gate has a target.
        // Force: the fix branch is the orchestrator's own throwaway, recreated
        // fresh off HEAD each attempt, so it must overwrite any stale branch a
        // prior (failed) attempt left on the remote — otherwise the fresh branch
        // is not a fast-forward and the push is rejected.
        let pushed = Command::new("git")
            .args(["push", "--force", "-u", "origin", branch])
            .current_dir(&self.repo_root)
            .status()
            .map_err(|e| LoopError(format!("git push: {e}")))?;
        if !pushed.success() {
            return Ok(false);
        }
        let body = if req.body.is_empty() {
            req.title.as_str()
        } else {
            req.body.as_str()
        };
        let pr = Command::new("gh")
            .args([
                "pr", "create", "--head", branch, "--title", &req.title, "--body", body,
            ])
            .current_dir(&self.repo_root)
            .status()
            .map_err(|e| LoopError(format!("gh pr create: {e}")))?;
        Ok(pr.success())
    }

    fn automerge(&self, branch: &str, req: &LoopRequest) -> Result<bool, LoopError> {
        let env = automerge::MergeEnvelope {
            enabled: true,
            allowlist: one(&req.repo),
            max_merges: 1,
        };
        let mreq = automerge::MergeRequest {
            branch: branch.to_string(),
            repo: req.repo.clone(),
        };
        let outcome = automerge::auto_merge(
            &automerge::GhChecksGate::default(),
            &automerge::GhMerger,
            &env,
            &mreq,
            0,
        )
        .map_err(|e| LoopError(e.0))?;
        Ok(matches!(outcome, automerge::MergeOutcome::Merged { .. }))
    }

    fn deploy(&self, branch: &str, req: &LoopRequest) -> Result<bool, LoopError> {
        let env = deploy::DeployEnvelope {
            enabled: true,
            allowlist: one(&req.repo),
            max_deploys: 1,
        };
        let dreq = deploy::DeployRequest {
            target: branch.to_string(),
            repo: req.repo.clone(),
        };
        let outcome = deploy::deploy(
            &deploy::CommandDeployer {
                canary_cmd: self.deploy_canary.clone(),
                promote_cmd: self.deploy_promote.clone(),
                rollback_cmd: self.deploy_rollback.clone(),
            },
            &deploy::CommandSmoke {
                smoke_cmd: self.deploy_smoke.clone(),
            },
            &env,
            &dreq,
            0,
        )
        .map_err(|e| LoopError(e.0))?;
        Ok(matches!(outcome, deploy::DeployOutcome::Promoted { .. }))
    }
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};

    use super::*;

    /// Records the stages reached, and returns configured per-stage results.
    struct FakeStages {
        dispatch_branch: Option<&'static str>,
        published: bool,
        merged: bool,
        promoted: bool,
        reached: RefCell<Vec<&'static str>>,
    }

    impl Stages for FakeStages {
        fn dispatch(&self, _req: &LoopRequest) -> Result<Option<String>, LoopError> {
            self.reached.borrow_mut().push("dispatch");
            Ok(self.dispatch_branch.map(String::from))
        }
        fn publish(&self, _b: &str, _req: &LoopRequest) -> Result<bool, LoopError> {
            self.reached.borrow_mut().push("publish");
            Ok(self.published)
        }
        fn automerge(&self, _b: &str, _req: &LoopRequest) -> Result<bool, LoopError> {
            self.reached.borrow_mut().push("automerge");
            Ok(self.merged)
        }
        fn deploy(&self, _b: &str, _req: &LoopRequest) -> Result<bool, LoopError> {
            self.reached.borrow_mut().push("deploy");
            Ok(self.promoted)
        }
    }

    fn repo() -> RepoId {
        RepoId {
            owner: "o".into(),
            name: "n".into(),
        }
    }
    fn req() -> LoopRequest {
        LoopRequest {
            issue: 8,
            title: "t".into(),
            body: "b".into(),
            repo: repo(),
        }
    }
    fn env(enabled: bool, allow: Vec<RepoId>, max: u32) -> LoopEnvelope {
        LoopEnvelope {
            enabled,
            allowlist: allow,
            max_iterations: max,
        }
    }
    fn stages(
        branch: Option<&'static str>,
        published: bool,
        merged: bool,
        promoted: bool,
    ) -> FakeStages {
        FakeStages {
            dispatch_branch: branch,
            published,
            merged,
            promoted,
            reached: RefCell::new(Vec::new()),
        }
    }

    #[test]
    fn all_green_completes_in_order() {
        let s = stages(Some("aom/fix/issue-8"), true, true, true);
        let r = run_loop(&s, &env(true, vec![repo()], 1), &req(), 0).unwrap();
        assert!(matches!(r, LoopOutcome::Completed { .. }));
        assert_eq!(
            *s.reached.borrow(),
            ["dispatch", "publish", "automerge", "deploy"]
        );
    }

    #[test]
    fn dispatch_none_stops_before_publish() {
        let s = stages(None, true, true, true);
        let r = run_loop(&s, &env(true, vec![repo()], 1), &req(), 0).unwrap();
        match r {
            LoopOutcome::Stopped { stage, .. } => assert_eq!(stage, "dispatch"),
            other => panic!("expected stop at dispatch, got {other:?}"),
        }
        assert_eq!(*s.reached.borrow(), ["dispatch"]);
    }

    #[test]
    fn unpublished_stops_before_merge() {
        let s = stages(Some("b"), false, true, true);
        let r = run_loop(&s, &env(true, vec![repo()], 1), &req(), 0).unwrap();
        match r {
            LoopOutcome::Stopped { stage, .. } => assert_eq!(stage, "publish"),
            other => panic!("expected stop at publish, got {other:?}"),
        }
        // Short-circuit: automerge/deploy never reached.
        assert_eq!(*s.reached.borrow(), ["dispatch", "publish"]);
    }

    #[test]
    fn unmerged_stops_before_deploy() {
        let s = stages(Some("b"), true, false, true);
        let r = run_loop(&s, &env(true, vec![repo()], 1), &req(), 0).unwrap();
        match r {
            LoopOutcome::Stopped { stage, .. } => assert_eq!(stage, "automerge"),
            other => panic!("expected stop at automerge, got {other:?}"),
        }
        assert_eq!(*s.reached.borrow(), ["dispatch", "publish", "automerge"]);
    }

    #[test]
    fn rolled_back_stops_at_deploy() {
        let s = stages(Some("b"), true, true, false);
        let r = run_loop(&s, &env(true, vec![repo()], 1), &req(), 0).unwrap();
        match r {
            LoopOutcome::Stopped { stage, .. } => assert_eq!(stage, "deploy"),
            other => panic!("expected stop at deploy, got {other:?}"),
        }
        assert_eq!(
            *s.reached.borrow(),
            ["dispatch", "publish", "automerge", "deploy"]
        );
    }

    #[test]
    fn kill_switch_refuses_before_any_stage() {
        let s = stages(Some("b"), true, true, true);
        let r = run_loop(&s, &env(false, vec![repo()], 1), &req(), 0).unwrap();
        assert!(matches!(r, LoopOutcome::Refused { .. }));
        assert!(s.reached.borrow().is_empty(), "no stage runs when disabled");
    }

    #[test]
    fn scope_fence_refuses_off_allowlist() {
        let s = stages(Some("b"), true, true, true);
        let r = run_loop(&s, &env(true, vec![], 1), &req(), 0).unwrap();
        match r {
            LoopOutcome::Refused { reason } => assert!(reason.contains("scope-fence")),
            other => panic!("expected scope-fence refusal, got {other:?}"),
        }
        assert!(s.reached.borrow().is_empty());
    }

    #[test]
    fn circuit_breaker_refuses_when_exhausted() {
        let s = stages(Some("b"), true, true, true);
        let r = run_loop(&s, &env(true, vec![repo()], 1), &req(), 1).unwrap();
        match r {
            LoopOutcome::Refused { reason } => assert!(reason.contains("circuit-breaker")),
            other => panic!("expected circuit-breaker refusal, got {other:?}"),
        }
        assert!(s.reached.borrow().is_empty());
    }

    #[test]
    fn audit_line_is_zero_pii() {
        let dir = tempfile::tempdir().unwrap();
        let outcome = LoopOutcome::Completed { branch: "b".into() };
        append_audit(dir.path(), &req(), &outcome, 100).unwrap();
        let body = std::fs::read_to_string(dir.path().join("loop.jsonl")).unwrap();
        let v: serde_json::Value = serde_json::from_str(body.trim()).unwrap();
        assert_eq!(v["action"], "loop");
        assert_eq!(v["issue"], 8);
        assert_eq!(v["repo"], "o/n");
        assert_eq!(v["outcome"], "completed");
        assert_eq!(v.as_object().unwrap().len(), 5);
    }

    /// Stages whose dispatch starts producing a branch only from attempt
    /// `dispatch_ok_from`; the other stages always advance. The call counter
    /// equals the number of loop iterations, for the multi-iteration assertions.
    struct CountingStages {
        dispatch_ok_from: u32,
        calls: Cell<u32>,
    }
    impl Stages for CountingStages {
        fn dispatch(&self, _req: &LoopRequest) -> Result<Option<String>, LoopError> {
            let n = self.calls.get();
            self.calls.set(n + 1);
            Ok((n >= self.dispatch_ok_from).then(|| "b".to_string()))
        }
        fn publish(&self, _b: &str, _req: &LoopRequest) -> Result<bool, LoopError> {
            Ok(true)
        }
        fn automerge(&self, _b: &str, _req: &LoopRequest) -> Result<bool, LoopError> {
            Ok(true)
        }
        fn deploy(&self, _b: &str, _req: &LoopRequest) -> Result<bool, LoopError> {
            Ok(true)
        }
    }

    #[test]
    fn until_done_completes_on_first_pass() {
        let s = CountingStages {
            dispatch_ok_from: 0,
            calls: Cell::new(0),
        };
        let r = run_until_done(&s, &env(true, vec![repo()], 3), &req()).unwrap();
        assert!(matches!(r, LoopOutcome::Completed { .. }));
        assert_eq!(s.calls.get(), 1, "landed on the first pass");
    }

    #[test]
    fn until_done_retries_then_completes() {
        let s = CountingStages {
            dispatch_ok_from: 1,
            calls: Cell::new(0),
        };
        let r = run_until_done(&s, &env(true, vec![repo()], 3), &req()).unwrap();
        assert!(matches!(r, LoopOutcome::Completed { .. }));
        assert_eq!(s.calls.get(), 2, "stopped once, retried, then landed");
    }

    #[test]
    fn until_done_exhausts_and_returns_last_stop() {
        let s = CountingStages {
            dispatch_ok_from: 99,
            calls: Cell::new(0),
        };
        let r = run_until_done(&s, &env(true, vec![repo()], 3), &req()).unwrap();
        match r {
            LoopOutcome::Stopped { stage, .. } => assert_eq!(stage, "dispatch"),
            other => panic!("expected the last stop, got {other:?}"),
        }
        assert_eq!(s.calls.get(), 3, "tried exactly max_iterations times");
    }

    #[test]
    fn until_done_does_not_retry_envelope_refusal() {
        let s = CountingStages {
            dispatch_ok_from: 0,
            calls: Cell::new(0),
        };
        let r = run_until_done(&s, &env(false, vec![repo()], 3), &req()).unwrap();
        assert!(matches!(r, LoopOutcome::Refused { .. }));
        assert_eq!(
            s.calls.get(),
            0,
            "kill-switch is terminal — no attempt, no retry"
        );
    }
}
