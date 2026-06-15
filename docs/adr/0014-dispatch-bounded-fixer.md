# ADR-0014 — Dispatch: a bounded hand-off to a fixer agent

## Status

Accepted (2026-06-27).

## Context

A3 turns an incident into a GitHub issue. A4 takes the next step: hand the issue
to a fixer agent that attempts a change. This is the orchestrator's first action
that *modifies code*, so the charter's safety envelope
(ADR: workspace-and-orchestrator-charter) starts to bind.

## Decision

`cosmatic dispatch` runs a single, hard-bounded attempt and stops at a proposed
branch. It never gates, merges, or deploys (that is A5).

- **`Fixer` trait** — `ClaudeFixer` (real, headless Claude Code) + `FakeFixer`
  (tests). All envelope logic is proven offline.
- **Envelope** — kill-switch (`cosmatic_DISPATCH_DISABLED`), scope-fence (a repo
  allowlist, defaulting to the target repo only), circuit-breaker (one attempt).
- **Isolation** — the fixer works in a throwaway git worktree on a fresh branch
  off `HEAD`; it never pushes, never opens a PR, never touches `main`.
- **Zero-PII audit** — every dispatch decision is journaled (action, issue,
  public repo coordinate, outcome, ts); no diffs, no paths, no authors.
- **Stops at a branch** — a human gates (`aom gate`/CI) and merges the proposed
  branch. Autonomy goes as far as *proposing* a fix, never landing it.

## Consequences

- The riskiest parts (the agent and the merge) stay under human control; A4 is a
  safe, reversible increment (close the issue, delete the branch).
- The `Fixer`/`Envelope` seam makes A5 (autonomous gate-and-merge) a matter of
  adding evidence checks and a merge step behind the same envelope, not a rewrite.
