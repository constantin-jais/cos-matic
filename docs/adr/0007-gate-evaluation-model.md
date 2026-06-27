# ADR-0007: Gate evaluation model

## Status

Accepted (2026-06-27).

## Context

A0 split the repo into a workspace with a `crates/orchestrator` scaffold. A1
adds the first real primitive of the agentic loop: a **gate-wall** that decides,
with attached proof, whether the project is in a mergeable/deployable state.
Every later phase depends on it (CI gate A2, fixer loop A4, autonomous merge A5).

## Decision

- **Declarative gates in `goals.toml`.** Hard gates (`[[gate]]`, blocking) and
  observability targets (`[[observe]]`, non-blocking) are each `metric op
threshold`. Operators: `eq | ne | lt | lte | gt | gte`. The gate set is data,
  not code.
- **Thresholds are integers; metric values are `f64`.** Integers parse cleanly
  from TOML and cover A1 (violation counts, a coverage percentage); `f64` values
  allow fractional metrics (e.g. coverage `84.21`). Comparisons happen in `f64`.
- **`CheckRunner` dependency injection.** The engine depends on a `CheckRunner`
  trait, not on `cargo`. Tests use a `FakeRunner`; production uses `CargoRunner`
  (shells out). This makes the engine fully unit-testable **and avoids a
  recursion trap**: a test that ran `cargo test` would re-enter itself.
- **Boolean check → violation count.** A1 maps each check to a 0/1 violation
  metric (`fmt_violations`, `clippy_violations`, `tests_failed`); `0` satisfies
  the `eq 0` gate. Parsing exact warning/failure counts is deferred.
- **Fail-closed.** `all_green` is true only if every hard gate is Green; a
  `Pending` hard gate (metric unavailable) does **not** pass. A gate-wall must
  never green-light what it could not verify (security axis #1).
- **Two commands.** `aom goals report` is the static "where are we" view
  (metrics Pending). `aom gate run` is the live enforcer: it runs the checks and
  exits non-zero on any red hard gate — the building block for the CI gate (A2)
  and the autonomous loop (A3+).

## Deferred (documented, not gold-plated)

- **Drift gate** (needs `agent_o_matic::generate --check`): added once the repo
  carries its own `harness.toml`; the orchestrator's dependency on the compiler
  is wired then.
- **Coverage metric** (needs a coverage tool): an `[[observe]]` row today,
  Pending until wired.
- **Failure detail / evidence**: `run_check` returns a `bool`; the _why_ of a
  failure becomes incident evidence in A3.

## Consequences

- The gate-wall is a pure, injectable engine plus a thin real runner — easy to
  test and to reuse from CI and from the fixer loop.
- `goals.toml` doubles as living documentation of where a phase stands.
