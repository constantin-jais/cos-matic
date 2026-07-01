# ADR-0017 — The end-to-end loop: one envelope, fail-safe by construction

## Status

Accepted (2026-06-27).

## Context

A3-A6 built the loop's links (incident -> issue -> dispatch -> automerge ->
deploy), each behind its own envelope. A7 chains them into a single `bolt-cosmatic loop`
under one global envelope.

## Decision

`bolt-cosmatic loop` runs dispatch -> automerge -> deploy and **stops at the first stage
that does not advance** — a later stage is never reached after an earlier one
stops. Fail-safe by construction.

- **Short-circuit** — no fix branch -> stop; not merged (gate not green) -> stop;
  rolled back (smoke not green) -> stop. Only a fully green chain completes.
- **Global envelope** — on top of each stage's own: a loop kill-switch
  (`BOLT_COSMATIC_LOOP_DISABLED`), a scope-fence, and a global circuit-breaker (max
  iterations).
- **Composable seam** — the three stages are a `Stages` trait, so the ordering,
  short-circuit, and envelope are proven offline; `RealStages` wires the real
  primitives (the live boundary).
- **Zero-PII audit** — every loop run is journaled.

## Consequences

- The autonomous CI/CD loop is complete and runnable as one command, with the
  whole charter envelope enforced at every link and globally.
- Each link stays usable standalone (`bolt-cosmatic incident|dispatch|automerge|deploy`);
  the loop is composition, not a monolith.
