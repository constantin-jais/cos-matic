# ADR-0022 — The loop retries: bounded multi-iteration

## Status

Accepted (2026-06-28).

## Context

`run_loop` ran exactly one pass, and the CLI set `max_iterations` to 1 — so the
circuit-breaker never bit and a single stop (no fix produced, a red gate, a
rolled-back deploy) ended everything. A real control loop retries.

## Decision

`run_until_done` wraps `run_loop`: it retries on every Stop until the loop
completes or the circuit-breaker (`max_iterations`, CLI default 3) is exhausted.

- **Bounded** — the circuit-breaker is the bound; the iteration count strictly
  increases, so termination is guaranteed (no runaway).
- **Envelope-terminal** — a kill-switch or scope-fence refusal is NOT retried;
  retrying a disabled or out-of-scope loop is pointless and unsafe.
- **Informative exhaustion** — when the budget is spent, the *last stop* (its
  stage and reason) is returned, not a bare "max iterations", so the caller learns
  why the loop never landed.
- Proven offline (completes-first-pass, retries-then-completes,
  exhausts-returns-last-stop, refusal-not-retried).

## Consequences

- `max_iterations` finally means something: the loop is a real, bounded retry
  cycle, not a one-shot.
- Each retry currently re-runs the same stages; for genuinely *distinct* attempts
  the live `RealStages` should vary the fix branch per iteration (and ideally feed
  the failure back to the fixer) — a live-boundary follow-up, out of scope here.
