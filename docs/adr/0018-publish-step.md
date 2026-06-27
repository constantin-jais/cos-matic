# ADR-0018 — Publish: the missing step that lets the loop complete

## Status

Accepted (2026-06-27).

## Context

Reviewing the never-run live paths (via the `aom loop --dry-run` added alongside)
exposed a gap: `dispatch` produces a *local* branch, but `automerge` gates a *PR*.
With nothing in between, the loop always stopped at automerge — `gh pr checks`
found no PR (verdict Unknown, fail-closed). The loop could not complete.

## Decision

Add a `publish` stage between dispatch and automerge: push the branch and open a
PR (`git push` + `gh pr create`). The loop is now dispatch -> publish -> automerge
-> deploy.

- **Distinct from dispatch.** Dispatch's charter (ADR: dispatch-bounded-fixer) is
  "never pushes, never opens a PR" — so publish is its own stage, not folded in.
  Run standalone, dispatch still stops at a local branch.
- **Still short-circuiting.** A failed push or PR stops the loop at `publish`;
  automerge and deploy are never reached. Same fail-safe contract.
- **Live boundary.** `RealStages::publish` shells out to git/gh; the offline test
  (`unpublished_stops_before_merge`) proves the composition without the network.

## Consequences

- The loop can now actually complete end-to-end: a green PR flows dispatch ->
  publish -> automerge -> deploy. The dry-run's reported gap is closed.
- Publishing is where the fix becomes a real, reviewable PR — the human review
  surface is preserved even on the autonomous path.
