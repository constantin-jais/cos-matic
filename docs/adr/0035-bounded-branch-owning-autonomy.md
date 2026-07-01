# ADR-0035 — Bounded branch-owning autonomy

## Status

Accepted (2026-07-01).

## Context

The product goal is not a passive dry-run assistant. Bolt-Cos-Matic should be
able to run an autonomous repository loop in a sandbox: create an issue when
needed, create one or more candidate branches for a task, compare attempts,
select a candidate, open or update a PR, and clean up branches it owns.

The dangerous wording is "fully autonomous". A repository agent must never own
the repository. It may own its ephemeral workspace; humans and GitHub protections
continue to own the repository, secrets, settings, protected branches, and merge
policy.

## Decision

Define the target capability as **bounded branch-owning autonomy**:

> The agent may create, update, compare, and delete branches inside an explicit
> agent-owned namespace. It may create issues and PRs. It must not modify
> repository settings, secrets, visibility, branch protections, protected
> branches, human branches, or bypass merge gates.

The default owned branch namespace is:

```text
bolt/
```

Structured attempts should use:

```text
bolt/run/<run-id>/issue-<issue>/attempt-<n>
```

The agent may delete only branches that pass the same owned-namespace policy.
Repository deletion is not an API exposed by Bolt-Cos-Matic and must not be
covered by any token used for live sandbox operation.

## Autonomy levels

- **L0 — observe:** dry-run, inspect, no writes.
- **L1 — branch autonomy:** create/push/delete agent-owned branches only.
- **L2 — PR autonomy:** open/update/close PRs for agent-owned branches.
- **L3 — gate-respecting merge:** merge only when normal GitHub protections and
  required checks allow it; no admin bypass.
- **L4 — multi-attempt autonomy:** create multiple candidate branches, score
  them, select one, close/clean up the rest, and record evidence.

The product target is L4 **inside the owned namespace**, not repository-admin
autonomy.

## Token model

A live sandbox needs a fine-grained GitHub PAT only because the agent must perform
real GitHub writes. The token should be scoped to the disposable sandbox repo and
limited to:

- Contents: read/write;
- Issues: read/write;
- Pull requests: read/write.

It must not have administration, secrets, actions-admin, organization-wide,
packages, or billing permissions.

GitHub's permission model does not reliably restrict a PAT to a branch prefix, so
Bolt-Cos-Matic must enforce branch ownership in code, while GitHub branch
protections enforce repository ownership boundaries.

## Safety invariants

- Never push to `main`, `master`, `develop`, or release branches.
- Never delete protected or human branches.
- Never modify repo settings, branch protection, secrets, deploy keys, billing,
  visibility, or Actions permissions.
- Never bypass required checks or review gates.
- Every write-capable run must have a kill switch and circuit breaker.
- Every autonomous action must be audit-recorded without secrets or personal data.
- Public harness live mode remains deterministic stub-only until stronger fixer
  isolation and policy are implemented.

## Implementation notes

`crates/orchestrator/src/branch_policy.rs` defines the first offline-enforced
branch ownership policy. It validates create/push/delete branch names before any
future live operation should touch GitHub. The real dispatch path now emits
structured attempt branches and the loop validates the branch before publish,
automerge, or deploy.

`crates/orchestrator/src/branch_gc.rs` defines the first offline garbage
collection planner. It only plans deletion for branches with matching ownership
metadata, expired TTL, delete-policy approval, and available deletion budget.

This ADR does not yet implement multi-attempt scoring, PR selection, or live
branch deletion. Those are follow-up capabilities built on the branch ownership
contract.

## Consequences

- "Autonomous" now means autonomous inside a bounded workspace, not repository
  administration.
- Tokens are optional until live sandbox tests are intentionally run.
- The public harness can stay secret-free by default.
- Future branch cleanup is safe only if it uses the same ownership policy.
