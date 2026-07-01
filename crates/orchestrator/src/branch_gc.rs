//! Offline garbage-collection planning for agent-owned branches.
//!
//! This module deliberately does not call GitHub or `git`. It only decides which
//! observed remote branches are eligible for deletion. A live deleter must apply
//! this plan and still validate each branch with `BranchPolicy::validate_delete`.

use crate::branch_policy::{AttemptBranch, BranchPolicy, BranchPolicyError};

/// Ownership metadata recorded for one bounded autonomous attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttemptOwnership {
    /// Non-PII owner identifier, e.g. `bolt-cosmatic-bot`.
    pub owner: String,
    pub run_id: String,
    pub issue: u64,
    pub attempt: u32,
    /// Unix timestamp in seconds.
    pub created_at: u64,
    /// Time-to-live in seconds. `0` means eligible immediately.
    pub ttl_seconds: u64,
}

impl AttemptOwnership {
    pub fn new(
        owner: impl Into<String>,
        run_id: impl Into<String>,
        issue: u64,
        attempt: u32,
        created_at: u64,
        ttl_seconds: u64,
    ) -> Result<Self, BranchPolicyError> {
        let owner = owner.into();
        validate_owner(&owner)?;
        let owned = Self {
            owner,
            run_id: run_id.into(),
            issue,
            attempt,
            created_at,
            ttl_seconds,
        };
        // Validate run/attempt through the canonical branch constructor.
        let _ = owned.branch()?;
        Ok(owned)
    }

    pub fn branch(&self) -> Result<AttemptBranch, BranchPolicyError> {
        AttemptBranch::new(&self.run_id, self.issue, self.attempt)
    }

    pub fn expires_at(&self) -> u64 {
        self.created_at.saturating_add(self.ttl_seconds)
    }

    pub fn is_expired(&self, now: u64) -> bool {
        now >= self.expires_at()
    }
}

/// A branch observed on the remote and the ownership metadata the orchestrator
/// has for it, if any.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedBranch {
    pub name: String,
    pub ownership: Option<AttemptOwnership>,
}

impl ObservedBranch {
    pub fn owned(ownership: AttemptOwnership) -> Result<Self, BranchPolicyError> {
        Ok(Self {
            name: ownership.branch()?.as_str().to_string(),
            ownership: Some(ownership),
        })
    }

    pub fn unowned(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ownership: None,
        }
    }
}

/// Runtime envelope for a GC pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GcEnvelope {
    /// Kill-switch. Wire this to `BOLT_COSMATIC_GC_DISABLED=1` at the boundary.
    pub enabled: bool,
    pub now: u64,
    pub max_deletions: usize,
}

/// One branch-level GC decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GcDecision {
    Delete { branch: String, reason: String },
    Keep { branch: String, reason: String },
}

/// A pure deletion plan. The executor must still fail closed if the remote state
/// changes between planning and deletion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GcPlan {
    pub decisions: Vec<GcDecision>,
}

impl GcPlan {
    pub fn deletions(&self) -> Vec<&str> {
        self.decisions
            .iter()
            .filter_map(|d| match d {
                GcDecision::Delete { branch, .. } => Some(branch.as_str()),
                GcDecision::Keep { .. } => None,
            })
            .collect()
    }
}

pub fn plan_gc(policy: &BranchPolicy, env: &GcEnvelope, branches: &[ObservedBranch]) -> GcPlan {
    let mut deletions = 0usize;
    let mut decisions = Vec::with_capacity(branches.len());

    for observed in branches {
        if !env.enabled {
            decisions.push(keep(&observed.name, "gc disabled by kill-switch"));
            continue;
        }

        let Some(ownership) = &observed.ownership else {
            decisions.push(keep(&observed.name, "missing ownership metadata"));
            continue;
        };

        let expected = match ownership.branch() {
            Ok(branch) => branch.as_str().to_string(),
            Err(e) => {
                decisions.push(keep(
                    &observed.name,
                    &format!("invalid ownership metadata: {e}"),
                ));
                continue;
            }
        };
        if expected != observed.name {
            decisions.push(keep(
                &observed.name,
                "branch name does not match ownership metadata",
            ));
            continue;
        }

        if !ownership.is_expired(env.now) {
            decisions.push(keep(&observed.name, "ttl not expired"));
            continue;
        }

        if let Err(e) = policy.validate_delete(&observed.name) {
            decisions.push(keep(
                &observed.name,
                &format!("branch policy refused deletion: {e}"),
            ));
            continue;
        }

        if deletions >= env.max_deletions {
            decisions.push(keep(&observed.name, "max deletions reached"));
            continue;
        }

        deletions += 1;
        decisions.push(GcDecision::Delete {
            branch: observed.name.clone(),
            reason: "owned ttl expired".to_string(),
        });
    }

    GcPlan { decisions }
}

fn keep(branch: &str, reason: &str) -> GcDecision {
    GcDecision::Keep {
        branch: branch.to_string(),
        reason: reason.to_string(),
    }
}

fn validate_owner(owner: &str) -> Result<(), BranchPolicyError> {
    if owner.trim().is_empty() {
        return Err(BranchPolicyError::InvalidBranch(
            "owner must not be empty".to_string(),
        ));
    }
    if !owner
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    {
        return Err(BranchPolicyError::InvalidBranch(format!(
            "unsafe owner `{owner}`"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> BranchPolicy {
        BranchPolicy::bolt_default()
    }

    fn env(now: u64) -> GcEnvelope {
        GcEnvelope {
            enabled: true,
            now,
            max_deletions: 10,
        }
    }

    fn own(created_at: u64, ttl_seconds: u64) -> AttemptOwnership {
        AttemptOwnership::new("bolt-cosmatic-bot", "run-1", 42, 1, created_at, ttl_seconds).unwrap()
    }

    #[test]
    fn owned_expired_branch_is_planned_for_deletion() {
        let b = ObservedBranch::owned(own(100, 50)).unwrap();
        let plan = plan_gc(&policy(), &env(151), &[b]);
        assert_eq!(plan.deletions(), vec!["bolt/run/run-1/issue-42/attempt-1"]);
    }

    #[test]
    fn disabled_gc_keeps_everything() {
        let b = ObservedBranch::owned(own(100, 0)).unwrap();
        let plan = plan_gc(
            &policy(),
            &GcEnvelope {
                enabled: false,
                now: 999,
                max_deletions: 10,
            },
            &[b],
        );
        assert!(matches!(plan.decisions[0], GcDecision::Keep { .. }));
        assert!(plan.deletions().is_empty());
    }

    #[test]
    fn unowned_branch_is_never_deleted() {
        let plan = plan_gc(
            &policy(),
            &env(999),
            &[ObservedBranch::unowned("feature/human")],
        );
        assert!(plan.deletions().is_empty());
        assert!(matches!(plan.decisions[0], GcDecision::Keep { .. }));
    }

    #[test]
    fn non_expired_branch_is_kept() {
        let b = ObservedBranch::owned(own(100, 50)).unwrap();
        let plan = plan_gc(&policy(), &env(149), &[b]);
        assert!(plan.deletions().is_empty());
    }

    #[test]
    fn mismatched_metadata_is_kept() {
        let b = ObservedBranch {
            name: "bolt/run/other/issue-42/attempt-1".to_string(),
            ownership: Some(own(100, 0)),
        };
        let plan = plan_gc(&policy(), &env(999), &[b]);
        assert!(plan.deletions().is_empty());
        match &plan.decisions[0] {
            GcDecision::Keep { reason, .. } => assert!(reason.contains("does not match")),
            other => panic!("expected keep, got {other:?}"),
        }
    }

    #[test]
    fn delete_budget_is_respected() {
        let a = ObservedBranch::owned(own(100, 0)).unwrap();
        let b = ObservedBranch::owned(
            AttemptOwnership::new("bolt-cosmatic-bot", "run-2", 42, 1, 100, 0).unwrap(),
        )
        .unwrap();
        let plan = plan_gc(
            &policy(),
            &GcEnvelope {
                enabled: true,
                now: 999,
                max_deletions: 1,
            },
            &[a, b],
        );
        assert_eq!(plan.deletions().len(), 1);
        assert!(matches!(plan.decisions[1], GcDecision::Keep { .. }));
    }

    #[test]
    fn invalid_owner_is_rejected() {
        let err = AttemptOwnership::new("human@example.com", "run", 1, 1, 0, 0).unwrap_err();
        assert!(err.to_string().contains("unsafe owner"));
    }
}
