# ADR-0038 — Solo-maintainer branch governance

## Status

Accepted (2026-07-02).

## Context

This repository currently has one human maintainer. A mandatory external review
requirement therefore creates a structural deadlock: the maintainer can produce
code, fix review blockers, and wait for all checks to pass, but cannot satisfy a
second independent approval without using a fictitious account or forging review
evidence.

PR #64 exposed that mismatch. The pull request was mergeable, all required checks
were green, auto-merge was enabled, and review blockers had been addressed. The
remaining blocker was only `REVIEW_REQUIRED` from branch protection.

The governance model should match the actual maintenance model: solo-maintainer
with mandatory automated evidence, not mandatory human approval that cannot be
fulfilled honestly.

This is a repository-maintainer policy decision. It does not expand the product
scope for autonomous agents: ADR-0035 still forbids Bolt-Cos-Matic live agents
from modifying repository settings, secrets, protected branches, or merge gates.

## Decision

Adopt a **solo-maintainer with mandatory checks** branch governance model for
`main`.

`main` remains protected. The required safeguards are:

- strict required status checks before merge;
- required conversation resolution;
- branch protection enforced for administrators;
- no force-pushes to `main`;
- no branch deletion for `main`;
- no forged reviews or second-account approvals.

The repository does **not** require pull request approval while it has only one
maintainer. Auto-merge is allowed once GitHub reports that all required checks and
branch protections are satisfied.

The current required status checks are:

- `Rust quality gates`;
- `Coverage floor`;
- `Rust supply-chain gates`;
- `Canvas handoff contract smoke`;
- `Dogfood drift gate`.

If a second maintainer with real review capacity joins the project, the review
requirement should be reconsidered. Re-enabling reviews is a governance change,
not an emergency fix.

## Consequences

- Solo maintenance no longer depends on dishonest review workarounds.
- CI and branch protection become the mandatory merge evidence.
- The main residual risk is the lack of independent human review.
- That risk is mitigated by small pull requests, ADRs for non-obvious decisions,
  required CI gates, required conversation resolution, and optional external
  review for high-risk changes.
- The policy is reversible by restoring required pull request reviews on `main`.

## Implementation notes

On 2026-07-02, `main` branch protection was changed after PR #64 was confirmed to
have green checks and no unresolved review threads.

Before the change, `main` required pull request reviews with:

- `required_approving_review_count = 1`;
- `require_code_owner_reviews = true`;
- `dismiss_stale_reviews = true`.

After the change:

- `required_pull_request_reviews = null`;
- `required_status_checks.strict = true` remains enabled;
- all five required status checks remain configured;
- `enforce_admins = true` remains enabled;
- `required_conversation_resolution = true` remains enabled;
- `allow_force_pushes = false` remains enabled;
- `allow_deletions = false` remains enabled;
- repository rulesets remain empty.

The administrative command used for the review requirement was:

```sh
gh api -X DELETE \
  repos/constantin-jais/bolt-cos-matic/branches/main/protection/required_pull_request_reviews \
  --silent
```

## Non-goals

- Does not disable required status checks.
- Does not allow force-pushes, deletion, or direct unprotected changes to `main`.
- Does not grant repository administration permissions to autonomous agents.
- Does not permit fake accounts, forged reviews, or fabricated approval evidence.
