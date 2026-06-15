//! Deploy: the loop's final safety step — canary -> smoke -> (promote | auto
//! rollback). The cardinal rule: a canary that fails (or cannot prove) smoke is
//! **always rolled back, never promoted**. Behind the binding envelope
//! (kill-switch, scope-fence, rate-limit) with a zero-PII audit. `Deployer` and
//! `Smoke` are traits, so the decision is proven offline; the real deploy and
//! probe commands are the live boundary. ADR: workspace-and-orchestrator-charter
//! is fully binding (this is a deploy).

use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

use serde::Serialize;

use crate::forge::RepoId;

/// A version/ref proposed for deploy.
#[derive(Debug, Clone)]
pub struct DeployRequest {
    pub target: String,
    pub repo: RepoId,
}

/// A handle to a deployed canary.
#[derive(Debug, Clone)]
pub struct Canary {
    pub id: String,
}

/// The smoke verdict on a canary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SmokeResult {
    Pass,
    Fail { reasons: Vec<String> },
}

/// A deploy or probe failure.
#[derive(Debug)]
pub struct DeployError(pub String);

impl std::fmt::Display for DeployError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "deploy error: {}", self.0)
    }
}

impl std::error::Error for DeployError {}

/// Deploys a canary, then promotes or rolls it back. Real impl shells out to a
/// configured deploy command; a fake is used in tests.
pub trait Deployer {
    fn deploy_canary(&self, req: &DeployRequest) -> Result<Canary, DeployError>;
    fn promote(&self, canary: &Canary) -> Result<String, DeployError>;
    fn rollback(&self, canary: &Canary) -> Result<(), DeployError>;
}

/// Smoke-checks a canary. Real impl runs a probe command (exit 0 = pass).
pub trait Smoke {
    fn check(&self, canary: &Canary) -> Result<SmokeResult, DeployError>;
}

/// The binding envelope for a deploy (ADR: workspace-and-orchestrator-charter).
#[derive(Debug, Clone)]
pub struct DeployEnvelope {
    /// Kill-switch: when false, every deploy is refused.
    pub enabled: bool,
    /// Scope-fence: only repos on this allowlist may be deployed.
    pub allowlist: Vec<RepoId>,
    /// Rate-limit / circuit-breaker: max deploys per run.
    pub max_deploys: u32,
}

/// The result: refused by the envelope, promoted (smoke passed), or rolled back
/// (smoke failed or could not be proven).
#[derive(Debug)]
pub enum DeployOutcome {
    Refused { reason: String },
    Promoted { reference: String },
    RolledBack { reason: String },
}

impl DeployOutcome {
    fn outcome(&self) -> &'static str {
        match self {
            DeployOutcome::Refused { .. } => "refused",
            DeployOutcome::Promoted { .. } => "promoted",
            DeployOutcome::RolledBack { .. } => "rolled_back",
        }
    }
}

/// Canary-deploy inside the envelope, then promote on a passing smoke or **roll
/// back on anything else** (a failing smoke, or a smoke that errored). A canary
/// is never left promoted without a passing smoke.
pub fn deploy<D: Deployer + ?Sized, S: Smoke + ?Sized>(
    deployer: &D,
    smoke: &S,
    env: &DeployEnvelope,
    req: &DeployRequest,
    deploys_this_run: u32,
) -> Result<DeployOutcome, DeployError> {
    if !env.enabled {
        return Ok(DeployOutcome::Refused {
            reason: "deploy is disabled (kill-switch)".to_string(),
        });
    }
    if !env.allowlist.contains(&req.repo) {
        return Ok(DeployOutcome::Refused {
            reason: format!(
                "scope-fence: {}/{} is not on the deploy allowlist",
                req.repo.owner, req.repo.name
            ),
        });
    }
    if deploys_this_run >= env.max_deploys {
        return Ok(DeployOutcome::Refused {
            reason: format!(
                "rate-limit: {deploys_this_run} deploy(s) already this run (max {})",
                env.max_deploys
            ),
        });
    }

    let canary = deployer.deploy_canary(req)?;

    // CARDINAL RULE: only a passing smoke promotes; everything else rolls back.
    match smoke.check(&canary) {
        Ok(SmokeResult::Pass) => {
            let reference = deployer.promote(&canary)?;
            Ok(DeployOutcome::Promoted { reference })
        }
        Ok(SmokeResult::Fail { reasons }) => {
            deployer.rollback(&canary)?;
            Ok(DeployOutcome::RolledBack {
                reason: format!("smoke failed, rolled back: {}", reasons.join("; ")),
            })
        }
        Err(e) => {
            // Smoke could not be proven — fail-closed: never leave the canary up.
            deployer.rollback(&canary)?;
            Ok(DeployOutcome::RolledBack {
                reason: format!("smoke could not be proven, rolled back: {e}"),
            })
        }
    }
}

/// A zero-PII audit record of a deploy decision.
#[derive(Serialize)]
struct AuditEntry<'a> {
    action: &'a str,
    repo: String,
    target: String,
    outcome: &'a str,
    ts: u64,
}

/// Append a zero-PII audit line for a deploy decision — every autonomous action
/// is recorded (ADR: workspace-and-orchestrator-charter).
pub fn append_audit(
    dir: &Path,
    req: &DeployRequest,
    outcome: &DeployOutcome,
    ts: u64,
) -> std::io::Result<()> {
    use std::io::Write as _;
    std::fs::create_dir_all(dir)?;
    let entry = AuditEntry {
        action: "deploy",
        repo: format!("{}/{}", req.repo.owner, req.repo.name),
        target: req.target.clone(),
        outcome: outcome.outcome(),
        ts,
    };
    let line = serde_json::to_string(&entry).map_err(std::io::Error::other)?;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("deploy.jsonl"))?;
    writeln!(f, "{line}")
}

/// Real deployer: each step shells out to a configured command (sovereign and
/// portable — a Clever Cloud canary, a script, whatever). `cosmatic_TARGET` and
/// `cosmatic_CANARY` are exported to the commands. Subprocess, so it is exercised
/// live, not in unit tests.
pub struct CommandDeployer {
    pub canary_cmd: String,
    pub promote_cmd: String,
    pub rollback_cmd: String,
}

impl Deployer for CommandDeployer {
    fn deploy_canary(&self, req: &DeployRequest) -> Result<Canary, DeployError> {
        run(&self.canary_cmd, &[("cosmatic_TARGET", req.target.as_str())])?;
        Ok(Canary {
            id: req.target.clone(),
        })
    }
    fn promote(&self, canary: &Canary) -> Result<String, DeployError> {
        run(&self.promote_cmd, &[("cosmatic_CANARY", canary.id.as_str())])?;
        Ok(format!("promoted {}", canary.id))
    }
    fn rollback(&self, canary: &Canary) -> Result<(), DeployError> {
        run(&self.rollback_cmd, &[("cosmatic_CANARY", canary.id.as_str())])
    }
}

/// Real smoke: a probe command; exit 0 is a pass, non-zero a fail.
pub struct CommandSmoke {
    pub smoke_cmd: String,
}

impl Smoke for CommandSmoke {
    fn check(&self, canary: &Canary) -> Result<SmokeResult, DeployError> {
        match run(&self.smoke_cmd, &[("cosmatic_CANARY", canary.id.as_str())]) {
            Ok(()) => Ok(SmokeResult::Pass),
            Err(e) => Ok(SmokeResult::Fail { reasons: vec![e.0] }),
        }
    }
}

fn run(cmd: &str, envs: &[(&str, &str)]) -> Result<(), DeployError> {
    if cmd.trim().is_empty() {
        return Err(DeployError("command is empty (configure it)".to_string()));
    }
    let mut env_map: HashMap<&str, &str> = HashMap::new();
    for (k, v) in envs {
        env_map.insert(k, v);
    }
    let out = Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .envs(env_map)
        .output()
        .map_err(|e| DeployError(format!("spawn `{cmd}`: {e}")))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(DeployError(format!(
            "`{cmd}` exited {}: {}",
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        )))
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    /// Records the sequence of deployer calls for cardinal-rule assertions.
    #[derive(Default)]
    struct FakeDeployer {
        calls: RefCell<Vec<&'static str>>,
    }
    impl Deployer for FakeDeployer {
        fn deploy_canary(&self, _req: &DeployRequest) -> Result<Canary, DeployError> {
            self.calls.borrow_mut().push("canary");
            Ok(Canary { id: "c1".into() })
        }
        fn promote(&self, _c: &Canary) -> Result<String, DeployError> {
            self.calls.borrow_mut().push("promote");
            Ok("prod".into())
        }
        fn rollback(&self, _c: &Canary) -> Result<(), DeployError> {
            self.calls.borrow_mut().push("rollback");
            Ok(())
        }
    }

    struct FakeSmoke(Result<SmokeResult, ()>);
    impl Smoke for FakeSmoke {
        fn check(&self, _c: &Canary) -> Result<SmokeResult, DeployError> {
            self.0
                .clone()
                .map_err(|()| DeployError("probe down".into()))
        }
    }

    fn repo() -> RepoId {
        RepoId {
            owner: "o".into(),
            name: "n".into(),
        }
    }
    fn req() -> DeployRequest {
        DeployRequest {
            target: "v1".into(),
            repo: repo(),
        }
    }
    fn env(enabled: bool, allow: Vec<RepoId>, max: u32) -> DeployEnvelope {
        DeployEnvelope {
            enabled,
            allowlist: allow,
            max_deploys: max,
        }
    }

    #[test]
    fn smoke_pass_promotes() {
        let d = FakeDeployer::default();
        let r = deploy(
            &d,
            &FakeSmoke(Ok(SmokeResult::Pass)),
            &env(true, vec![repo()], 1),
            &req(),
            0,
        )
        .unwrap();
        assert!(matches!(r, DeployOutcome::Promoted { .. }));
        assert_eq!(*d.calls.borrow(), ["canary", "promote"]);
    }

    #[test]
    fn smoke_fail_rolls_back_never_promotes() {
        let d = FakeDeployer::default();
        let smoke = FakeSmoke(Ok(SmokeResult::Fail {
            reasons: vec!["503".into()],
        }));
        let r = deploy(&d, &smoke, &env(true, vec![repo()], 1), &req(), 0).unwrap();
        assert!(matches!(r, DeployOutcome::RolledBack { .. }));
        // CARDINAL: canary then rollback, and promote was NEVER called.
        assert_eq!(*d.calls.borrow(), ["canary", "rollback"]);
    }

    #[test]
    fn smoke_error_rolls_back_fail_closed() {
        let d = FakeDeployer::default();
        let r = deploy(
            &d,
            &FakeSmoke(Err(())),
            &env(true, vec![repo()], 1),
            &req(),
            0,
        )
        .unwrap();
        assert!(matches!(r, DeployOutcome::RolledBack { .. }));
        assert_eq!(*d.calls.borrow(), ["canary", "rollback"]);
    }

    #[test]
    fn kill_switch_refuses_before_deploying() {
        let d = FakeDeployer::default();
        let r = deploy(
            &d,
            &FakeSmoke(Ok(SmokeResult::Pass)),
            &env(false, vec![repo()], 1),
            &req(),
            0,
        )
        .unwrap();
        assert!(matches!(r, DeployOutcome::Refused { .. }));
        assert!(
            d.calls.borrow().is_empty(),
            "nothing deployed when disabled"
        );
    }

    #[test]
    fn scope_fence_refuses_off_allowlist() {
        let d = FakeDeployer::default();
        let r = deploy(
            &d,
            &FakeSmoke(Ok(SmokeResult::Pass)),
            &env(true, vec![], 1),
            &req(),
            0,
        )
        .unwrap();
        match r {
            DeployOutcome::Refused { reason } => assert!(reason.contains("scope-fence")),
            other => panic!("expected scope-fence refusal, got {other:?}"),
        }
        assert!(d.calls.borrow().is_empty());
    }

    #[test]
    fn rate_limit_refuses_when_exhausted() {
        let d = FakeDeployer::default();
        let r = deploy(
            &d,
            &FakeSmoke(Ok(SmokeResult::Pass)),
            &env(true, vec![repo()], 1),
            &req(),
            1,
        )
        .unwrap();
        match r {
            DeployOutcome::Refused { reason } => assert!(reason.contains("rate-limit")),
            other => panic!("expected rate-limit refusal, got {other:?}"),
        }
        assert!(d.calls.borrow().is_empty());
    }

    #[test]
    fn audit_line_is_zero_pii() {
        let dir = tempfile::tempdir().unwrap();
        let outcome = DeployOutcome::RolledBack { reason: "r".into() };
        append_audit(dir.path(), &req(), &outcome, 100).unwrap();
        let body = std::fs::read_to_string(dir.path().join("deploy.jsonl")).unwrap();
        let v: serde_json::Value = serde_json::from_str(body.trim()).unwrap();
        assert_eq!(v["action"], "deploy");
        assert_eq!(v["repo"], "o/n");
        assert_eq!(v["outcome"], "rolled_back");
        assert_eq!(v.as_object().unwrap().len(), 5);
    }
}
