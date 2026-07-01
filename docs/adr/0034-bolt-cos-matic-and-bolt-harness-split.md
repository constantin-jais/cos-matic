# ADR-0034 — Rename to Bolt-Cos-Matic and split Bolt Harness

## Status

Accepted (2026-07-01).

## Context

The project is the **Bolt** orchestration layer of the ecosystem: it turns product
intent into safe, inspectable plans, policy-gated actions, and execution evidence.
The previous `cos-matic` name was usable but too generic; it did not make the
Bolt-layer ownership explicit.

In parallel, the historical `aom-sandbox` repository had become ambiguous. It
looked like a fork or old copy of the engine, while its actual value was as a
throwaway place to exercise the autonomous loop in live CI. Keeping engine code
in a sandbox creates two sources of truth and makes public trust harder: readers
cannot tell which repository is canonical.

## Decision

Rename the canonical engine to **Bolt-Cos-Matic**:

- repository: `bolt-cos-matic`;
- CLI: `bolt-cosmatic`;
- core package: `bolt-cos-matic` / Rust crate `bolt_cos_matic`;
- internal policy directory: `.bolt-cosmatic`;
- environment variables: `BOLT_COSMATIC_*`;
- sandbox guard variable: `BOLT_HARNESS_SANDBOX`.

Split the public proof bench into **Bolt Harness**:

- `bolt-cos-matic` owns durable logic: compiler, safe-write, goals, inspect,
  handoff, maturity, orchestrator traits, gates, policies, ADRs, and tests;
- `bolt-harness` owns reproducible scenarios: fixtures, CI workflows, smoke
  scripts, and scrubbed evidence;
- `bolt-harness` must install or call `bolt-cosmatic`; it must not copy engine
  crates or orchestration internals.

The historical `aom-sandbox` is superseded by `bolt-harness` and should be kept
as private/archive history or replaced by the clean public harness.

## Consequences

- The ecosystem map is clearer: Bolt-Cos-Matic is the Bolt brain; Bolt Harness is
  the bounded demonstration environment.
- Public audit improves because the canonical engine is open and the sandbox is
  explicitly not the source of truth.
- Migration touches package names, binary names, generated docs, CI templates,
  environment variables, branch prefixes, audit directories, and tests.
- Short-term compatibility is intentionally limited because the project is still
  pre-stable. Avoid carrying legacy `aom`/`cosmatic` aliases unless adoption data
  requires them.
- Any future live-demo repository should be created from `bolt-harness`, not by
  copying the engine repository.
