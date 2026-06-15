# ADR-0010 — Drift detection as a CI gate

- Status: accepted
- Date: 2026-06-27

## Context

`cosmatic generate` is deterministic and uses safe-write, so a project can commit its
generated outputs (`AGENTS.md`, `CLAUDE.md`, `.cursor/rules/*`, …) as **golden
files**. The remaining risk is that someone edits the source (`harness.toml` or a
domain `.md`) and forgets to regenerate, so the committed outputs no longer match
the source. That divergence is _drift_.

`cosmatic generate --check` already detects drift (it re-renders in memory and compares
to disk, writing nothing). Phase 5 turns that into an enforced gate.

## Decision

- **`--check` reports every drifted file in one pass**, not just the first, so a
  CI run shows all the work needed at once. It exits nonzero (`Error::Drift` with
  the full list) when anything diverged; a real IO error still surfaces as itself.
- Ship a **reusable CI workflow** (`ci-templates/cos-matic.yml`) that installs
  the tool and runs `cosmatic generate --check` and `cosmatic goals`. Adopters copy it.
- **Dogfood it:** the project's own CI builds `aom` and runs `cosmatic generate --check`
  plus `cosmatic goals` against `examples/minimal`, so the repository fails CI if the
  committed example outputs drift or a hard gate breaks.

## Consequences

- The gate composes with goals: `cosmatic generate --check` also enforces hard gates
  (goals run before the write/check path), so one command covers both.
- The golden-file model is now load-bearing: outputs are committed and protected.
  The audit log (`.harness/audit.jsonl`) stays gitignored (timestamps); the
  lockfile (`.harness/lock.toml`) is committed.
- No new runtime surface or dependency: this is the existing `--check` made
  exhaustive plus CI plumbing. A scaffolding command (`cosmatic init-ci`) remains a
  future convenience, not required for the gate.
