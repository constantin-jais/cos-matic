# ADR-0015 — Autonomous merge: gate-and-merge on green evidence only

## Status

Accepted (2026-06-27).

## Context

A4 produces a proposed fix branch but stops there. A5 closes the loop: it can
land a branch autonomously. This is the action that *modifies main*, so the
charter's safety envelope (ADR: workspace-and-orchestrator-charter) is fully
binding here.

## Decision

`cosmatic automerge` gates-and-merges a branch, governed by one cardinal rule and the
binding envelope:

- **Cardinal rule — nothing merges without attached green evidence.** The gate
  yields a `Verdict`; only `Green` may merge. `Red` and `Unknown` (pending or
  missing checks) both refuse — **fail-closed**. A gate-wall never green-lights
  what it could not verify.
- **Envelope** — kill-switch (`cosmatic_AUTOMERGE_DISABLED`), scope-fence (a repo
  allowlist), rate-limit / circuit-breaker (max merges per run).
- **Reversible** — a merge is undoable by revert; combined with the green-only
  rule, the blast radius stays bounded.
- **Seams** — `Gate` and `Merger` are traits, so the decision logic is proven
  offline (green->merge, red->never, unknown->fail-closed, plus every envelope
  refusal). The original live implementation used the GitHub CLI; ADR-0027 moved
  GitHub operations to the `Forge`/octocrab seam. The network is the live
  boundary.
- **Zero-PII audit** — every decision is journaled (action, repo, branch,
  outcome, ts).

## Consequences

- The autonomous loop is complete: incident -> issue -> dispatch (bounded fix) ->
  automerge (green-only). Each step is reversible and behind the same envelope.
- Deploy hardening (canary -> smoke -> rollback) is a separate, later concern;
  A5 lands code, it does not deploy.
