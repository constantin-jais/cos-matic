//! Dispatch: the bounded hand-off to a fixer agent. `dispatch` is the safety
//! envelope around an *untrusted* fix attempt — kill-switch, scope-fence, a
//! single attempt, zero-PII audit — and it stops at a proposed branch. It never
//! gates, merges, or deploys (that is A5); a human reviews the produced branch.

use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

use serde::Serialize;

use crate::forge::RepoId;

/// What the fixer is asked to address.
#[derive(Debug, Clone)]
pub struct FixRequest {
    pub issue: u64,
    pub title: String,
    pub body: String,
    pub repo: RepoId,
}

/// What a fixer produced: a branch carrying a proposed change, for human review.
#[derive(Debug, Clone)]
pub struct FixReport {
    pub branch: String,
    pub summary: String,
}

/// A fixer failure (spawn, non-zero exit, isolation error).
#[derive(Debug)]
pub struct FixerError(pub String);

impl std::fmt::Display for FixerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "fixer error: {}", self.0)
    }
}

impl std::error::Error for FixerError {}

/// A bounded attempt to fix an issue. Implemented by `ClaudeFixer` (real) and a
/// fake in tests; the trait keeps the dispatch envelope fully testable offline.
pub trait Fixer {
    fn attempt(&self, req: &FixRequest) -> Result<FixReport, FixerError>;
}

/// The hard envelope a dispatch must stay inside
/// (ADR: workspace-and-orchestrator-charter).
#[derive(Debug, Clone)]
pub struct Envelope {
    /// Kill-switch: when false, every dispatch is refused.
    pub enabled: bool,
    /// Scope-fence: only repos on this allowlist may be dispatched.
    pub allowlist: Vec<RepoId>,
    /// Circuit-breaker: max fix attempts per dispatch (1 in A4).
    pub max_attempts: u32,
}

/// The result of a dispatch: either refused by the envelope, or a single
/// bounded attempt. Never a merge or a deploy.
#[derive(Debug)]
pub enum DispatchReport {
    /// The envelope refused the dispatch (kill-switch or scope-fence).
    Refused { reason: String },
    /// The fixer produced a branch for human review (NOT gated, NOT merged).
    Attempted { branch: String, summary: String },
}

impl DispatchReport {
    fn outcome(&self) -> &'static str {
        match self {
            DispatchReport::Refused { .. } => "refused",
            DispatchReport::Attempted { .. } => "attempted",
        }
    }
}

/// Run a single bounded fix attempt inside the envelope. Envelope refusals are
/// reported (not errors); only a fixer failure is an error. Pure orchestration:
/// the side effects (isolation, the agent) live in the `Fixer`.
pub fn dispatch<F: Fixer + ?Sized>(
    fixer: &F,
    env: &Envelope,
    req: &FixRequest,
) -> Result<DispatchReport, FixerError> {
    if !env.enabled {
        return Ok(DispatchReport::Refused {
            reason: "dispatch is disabled (kill-switch)".to_string(),
        });
    }
    if !env.allowlist.contains(&req.repo) {
        return Ok(DispatchReport::Refused {
            reason: format!(
                "scope-fence: {}/{} is not on the dispatch allowlist",
                req.repo.owner, req.repo.name
            ),
        });
    }
    // Circuit-breaker: A4 ships exactly one attempt; retries are an A5 concern.
    debug_assert!(env.max_attempts >= 1);
    let report = fixer.attempt(req)?;
    Ok(DispatchReport::Attempted {
        branch: report.branch,
        summary: report.summary,
    })
}

/// A zero-PII audit record of a dispatch decision. Records the public repo
/// coordinate and the outcome only — never issue authors, diffs, or paths.
#[derive(Serialize)]
struct AuditEntry<'a> {
    action: &'a str,
    issue: u64,
    repo: String,
    outcome: &'a str,
    ts: u64,
}

/// Append a zero-PII audit line for a dispatch decision — every autonomous
/// action is recorded (ADR: workspace-and-orchestrator-charter).
pub fn append_audit(
    dir: &Path,
    issue: u64,
    repo: &RepoId,
    report: &DispatchReport,
    ts: u64,
) -> std::io::Result<()> {
    use std::io::Write as _;
    std::fs::create_dir_all(dir)?;
    let entry = AuditEntry {
        action: "dispatch",
        issue,
        repo: format!("{}/{}", repo.owner, repo.name),
        outcome: report.outcome(),
        ts,
    };
    let line = serde_json::to_string(&entry).map_err(std::io::Error::other)?;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("dispatch.jsonl"))?;
    writeln!(f, "{line}")
}

/// The real fixer: hands the issue to headless Claude Code in an isolated git
/// worktree on a throwaway branch off `HEAD`. It never pushes, never opens a PR,
/// never touches `main`; the produced branch is left for a human to gate and
/// merge. Subprocess + filesystem, so it is exercised live, not in unit tests.
pub struct ClaudeFixer {
    pub repo_root: PathBuf,
}

impl Fixer for ClaudeFixer {
    fn attempt(&self, req: &FixRequest) -> Result<FixReport, FixerError> {
        let branch = format!("aom/fix/issue-{}", req.issue);
        let wt = self
            .repo_root
            .join(".aom")
            .join("dispatch")
            .join(format!("issue-{}", req.issue));
        let wt_str = wt.to_string_lossy().into_owned();

        // Isolate: a fresh worktree on a new branch off HEAD. Never on `main`.
        fresh_worktree(&self.repo_root, &branch, &wt_str)?;

        // Hand off to the fixer agent — one attempt, confined to the worktree.
        let prompt = format!(
            "Fix issue #{}: {}\n\n{}\n\nMake a minimal change: edit the files to fix \
             it; you may run `cargo` to check your work. Do NOT run git, commit, \
             push, or open a PR, and do not touch main — the change is committed for \
             you afterward.",
            req.issue, req.title, req.body
        );
        // Hardening (ADR: bounded-fixer-bash-hardening): the headless fixer gets an
        // allow-list, not arbitrary Bash. It may edit/read/search and run `cargo`
        // to verify — but no git, no network, no `rm`, no arbitrary shell.
        // `--permission-mode dontAsk` makes anything off the list fail closed
        // (auto-denied, never a prompt that would hang a headless run).
        let status = Command::new("claude")
            .args([
                "-p",
                &prompt,
                "--permission-mode",
                "dontAsk",
                "--allowedTools",
                "Edit",
                "Write",
                "Read",
                "Grep",
                "Glob",
                "Bash(cargo *)",
            ])
            .current_dir(&wt)
            .status()
            .map_err(|e| FixerError(format!("spawn claude: {e}")))?;
        if !status.success() {
            return Err(FixerError(format!(
                "fixer exited with {status}; worktree left at {wt_str} for inspection"
            )));
        }

        // The fixer only edits; commit its work so the branch is publishable. An
        // empty diff is a clear failure (no fix produced), not a silent empty PR.
        if git_stdout(&wt, &["status", "--porcelain"])?
            .trim()
            .is_empty()
        {
            return Err(FixerError(format!(
                "fixer produced no changes; worktree left at {wt_str} for inspection"
            )));
        }
        run_git(&wt, &["add", "-A"])?;
        commit_fix(&wt, &format!("fix: #{} {}", req.issue, req.title))?;

        Ok(FixReport {
            branch,
            summary: format!("fixer ran and committed in an isolated worktree ({wt_str})"),
        })
    }
}

fn run_git(root: &Path, args: &[&str]) -> Result<(), FixerError> {
    let out = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .map_err(|e| FixerError(format!("spawn git: {e}")))?;
    if !out.status.success() {
        return Err(FixerError(format!(
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(())
}

/// Create a fresh worktree + branch off HEAD, discarding any leftover from a
/// prior attempt. The loop may retry (ADR: loop-bounded-retry); a retry is a
/// clean re-do, so we remove the previous worktree/branch (best effort) before
/// recreating — otherwise `worktree add -b` fails with "branch already exists".
fn fresh_worktree(repo_root: &Path, branch: &str, wt: &str) -> Result<(), FixerError> {
    let _ = Command::new("git")
        .args(["worktree", "remove", "--force", wt])
        .current_dir(repo_root)
        .output();
    let _ = Command::new("git")
        .args(["branch", "-D", branch])
        .current_dir(repo_root)
        .output();
    run_git(repo_root, &["worktree", "add", "-b", branch, wt, "HEAD"])
}

/// Commit the fixer's staged work with a stable bot identity. A fresh CI runner
/// has no configured git user, so a bare `git commit` fails with "Author identity
/// unknown" — setting it per-invocation keeps the orchestrator self-contained
/// (works in CI and locally, without mutating global git config).
fn commit_fix(wt: &Path, message: &str) -> Result<(), FixerError> {
    run_git(
        wt,
        &[
            "-c",
            "user.name=aom-bot",
            "-c",
            "user.email=aom-bot@users.noreply.github.com",
            "commit",
            "-m",
            message,
        ],
    )
}

/// Like `run_git`, but returns stdout — used to inspect `git status` before
/// committing the fixer's work.
fn git_stdout(root: &Path, args: &[&str]) -> Result<String, FixerError> {
    let out = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .map_err(|e| FixerError(format!("spawn git: {e}")))?;
    if !out.status.success() {
        return Err(FixerError(format!(
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// A deterministic fixer with no LLM (and no Anthropic key): it makes one
/// trivial, safe change and commits it, so a real branch flows through
/// publish -> gate -> merge -> deploy. Used to validate the loop's plumbing on a
/// sandbox before (or without) wiring the real `ClaudeFixer`. Selected with
/// `AOM_FIXER=stub` (ADR: stub-fixer-for-plumbing).
pub struct StubFixer {
    pub repo_root: PathBuf,
}

impl Fixer for StubFixer {
    fn attempt(&self, req: &FixRequest) -> Result<FixReport, FixerError> {
        use std::io::Write as _;

        let branch = format!("aom/fix/issue-{}", req.issue);
        let wt = self
            .repo_root
            .join(".aom")
            .join("dispatch")
            .join(format!("issue-{}", req.issue));
        let wt_str = wt.to_string_lossy().into_owned();

        fresh_worktree(&self.repo_root, &branch, &wt_str)?;

        // The "fix": append an auditable marker line. Harmless and never breaks
        // the build, so the branch sails through CI and the green-only gate.
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(wt.join("SANDBOX_FIXES.md"))
            .map_err(|e| FixerError(format!("write marker: {e}")))?;
        writeln!(f, "- stub fix for issue #{}: {}", req.issue, req.title)
            .map_err(|e| FixerError(format!("write marker: {e}")))?;

        run_git(&wt, &["add", "-A"])?;
        commit_fix(&wt, &format!("fix: #{} {} (stub)", req.issue, req.title))?;

        Ok(FixReport {
            branch,
            summary: format!("stub fixer wrote SANDBOX_FIXES.md ({wt_str})"),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    /// Counts attempts so tests can assert the fixer is (not) invoked.
    struct FakeFixer {
        calls: Cell<u32>,
    }

    impl FakeFixer {
        fn new() -> Self {
            Self {
                calls: Cell::new(0),
            }
        }
    }

    impl Fixer for FakeFixer {
        fn attempt(&self, req: &FixRequest) -> Result<FixReport, FixerError> {
            self.calls.set(self.calls.get() + 1);
            Ok(FixReport {
                branch: format!("aom/fix/issue-{}", req.issue),
                summary: "fake fix".to_string(),
            })
        }
    }

    fn repo() -> RepoId {
        RepoId {
            owner: "o".into(),
            name: "n".into(),
        }
    }

    fn req() -> FixRequest {
        FixRequest {
            issue: 8,
            title: "t".into(),
            body: "b".into(),
            repo: repo(),
        }
    }

    fn env(enabled: bool, allow: Vec<RepoId>) -> Envelope {
        Envelope {
            enabled,
            allowlist: allow,
            max_attempts: 1,
        }
    }

    #[test]
    fn kill_switch_refuses_without_calling_the_fixer() {
        let f = FakeFixer::new();
        let r = dispatch(&f, &env(false, vec![repo()]), &req()).unwrap();
        assert!(matches!(r, DispatchReport::Refused { .. }));
        assert_eq!(f.calls.get(), 0, "fixer must not run when disabled");
    }

    #[test]
    fn scope_fence_refuses_repo_off_allowlist() {
        let f = FakeFixer::new();
        let r = dispatch(&f, &env(true, vec![]), &req()).unwrap();
        match r {
            DispatchReport::Refused { reason } => assert!(reason.contains("scope-fence")),
            other => panic!("expected a scope-fence refusal, got {other:?}"),
        }
        assert_eq!(f.calls.get(), 0, "fixer must not run off the allowlist");
    }

    #[test]
    fn allowed_dispatch_makes_a_single_attempt() {
        let f = FakeFixer::new();
        let r = dispatch(&f, &env(true, vec![repo()]), &req()).unwrap();
        match r {
            DispatchReport::Attempted { branch, .. } => assert_eq!(branch, "aom/fix/issue-8"),
            other => panic!("expected an attempt, got {other:?}"),
        }
        assert_eq!(f.calls.get(), 1, "exactly one attempt (circuit-breaker)");
    }

    #[test]
    fn audit_line_is_zero_pii_and_parseable() {
        let dir = tempfile::tempdir().unwrap();
        let report = DispatchReport::Attempted {
            branch: "b".into(),
            summary: "s".into(),
        };
        append_audit(dir.path(), 8, &repo(), &report, 100).unwrap();
        let body = std::fs::read_to_string(dir.path().join("dispatch.jsonl")).unwrap();
        let v: serde_json::Value = serde_json::from_str(body.trim()).unwrap();
        assert_eq!(v["action"], "dispatch");
        assert_eq!(v["issue"], 8);
        assert_eq!(v["repo"], "o/n");
        assert_eq!(v["outcome"], "attempted");
        // Only the controlled fields exist — no diff, no path, no author.
        assert_eq!(v.as_object().unwrap().len(), 5);
    }
}
