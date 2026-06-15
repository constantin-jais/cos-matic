# ADR-0016 — Deploy: canary -> smoke -> promote or auto-rollback

## Status

Accepted (2026-06-27).

## Context

A5 lands code on main. The charter's last safety clause is the deploy: changes
reach production only behind a canary that is smoke-tested and **automatically
rolled back on failure**. A6 implements that step.

## Decision

`cosmatic deploy` runs canary -> smoke -> (promote | rollback), governed by the
cardinal rule and the binding envelope:

- **Cardinal rule — a canary that fails (or cannot prove) smoke is always rolled
  back, never promoted.** Only a passing smoke promotes; a failing smoke and a
  smoke that errored both roll back (fail-closed).
- **Envelope** — kill-switch (`cosmatic_DEPLOY_DISABLED`), scope-fence (a repo
  allowlist), rate-limit / circuit-breaker (max deploys per run).
- **Reversible by construction** — rollback is part of the flow, not an
  afterthought; a bad deploy never stays up.
- **Seams** — `Deployer` (canary/promote/rollback) and `Smoke` (probe) are
  traits, proven offline (pass->promote, fail->rollback, error->rollback, plus
  every envelope refusal). The real impls shell out to configured commands
  (`cosmatic_DEPLOY_{CANARY,PROMOTE,ROLLBACK,SMOKE}`) — sovereign and portable; the
  process/network is the live boundary.
- **Zero-PII audit** — every deploy decision is journaled.

## Consequences

- The full charter envelope is now realized: hard gates (A5), reversible deploy
  with automatic rollback (A6), circuit-breaker, kill-switch, scope-fence, and a
  zero-PII audit across the whole loop.
- The deploy mechanism is a configured command, so the loop adapts to any target
  (Clever Cloud, a script) without changing the orchestrator.
