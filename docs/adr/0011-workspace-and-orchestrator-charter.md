# ADR-0011: Workspace split and the orchestrator charter

## Status

Accepted (2026-06-27).

## Context

Agent-O-Matic began as a single crate: a clean-room, deterministic
configuration compiler whose charter (ADR: positioning-and-why-build) scopes success to _learning in
order to teach_ — not adoption — and names a remote content registry, "19
targets on day one", and an early MCP server as gold-plating against that goal.
Agent orchestration is **not** in ADR: positioning-and-why-build's exclusion list; it was simply
outside the compiler's scope. It enters here as a new, orthogonal concern, not
as a retro-fitted exclusion.

A new, separate ambition has appeared: an **open-source agentic CI/CD loop**
(incident → issue → hand-off to a fixer agent → hard gates → merge → deploy →
verify → rollback), built _on top of_ the compiler and dogfooded by it. This
concern is orthogonal to "compile one source into many agent configs".

## Decision

Restructure the repository into a Cargo workspace rather than growing the
compiler crate:

- `crates/aom` — the compiler library `agent_o_matic`, **unchanged in spirit
  and still governed by ADR: positioning-and-why-build**. It gains no orchestration, no MCP, no
  network dependency. It loses only its CLI wiring (moved out), which sharpens
  its identity as a pure library.
- `crates/cli` — the `aom` binary; a thin application layer that wires the
  compiler (and, later, the orchestrator) behind a clap CLI.
- `crates/orchestrator` — the new concern: goals & gates (A1), then the
  incident/issue/dispatch loop (A3+). Its charter is separate from ADR: positioning-and-why-build.

ADR: positioning-and-why-build is therefore **not superseded**: it continues to describe the
compiler crate exactly. This ADR adds the workspace and a distinct charter for
the orchestrator.

## The orchestrator's safety envelope (binding from A5 onward)

Autonomy is permitted only inside a hard, reversible envelope:

1. Hard gates are blocking and evidence-backed (nothing merges/deploys without
   attached green proof).
2. Deploys are reversible with automatic rollback (canary → smoke → rollback).
3. A circuit-breaker bounds blast radius (deploy rate-limit, max fix attempts,
   global kill-switch).
4. Every autonomous action is recorded in a zero-PII audit trail.
5. A scope-fence restricts the loop to an allowlist of repos/targets; it never
   touches infrastructure credentials.

## Consequences

- One-time restructure cost; the compiler's tests and guarantees are unchanged
  (the existing suite is the regression gate).
- The compiler stays publishable and teachable on its own; the orchestrator can
  depend on it without polluting it.
- Profiles (`[profile.release]`) now live only in the workspace root, per Cargo.
- A workspace dependency references a crate by its **package** name
  (`agent-o-matic`), while Rust code imports it by its **library** name
  (`agent_o_matic`); the two differ here and the manifests reflect that.
