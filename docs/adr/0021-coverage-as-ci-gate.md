# ADR-0021 — Coverage as a blocking CI gate

## Status

Accepted (2026-06-28).

## Context

The quality bar requires a coverage report with a blocking threshold in CI, and
E2E tests on non-trivial logic. Until now there was neither — and the
orchestrator's CLI arms (`main.rs`) were exercised by no test at all.

## Decision

- A `coverage` CI job runs `cargo llvm-cov --workspace --fail-under-lines` — a
  report on every run with a **blocking floor**.
- The orchestrator's live-boundary wrappers (the gh/git/claude subprocess seams:
  the `GhChecksGate` poll loop, `GhMerger`, `GhPublisher`, `ClaudeFixer`,
  `GithubForge`, `CommandDeployer`, `RealStages`) are deliberately not
  unit-covered — they are the I/O boundary, proven live, not in tests, which caps
  the achievable figure below 100. Measured line coverage is ~85.5%; the floor is
  set at 80 — a small margin below, so it bites on a real regression without
  false-failing — and ratchets up as coverage grows.
- E2E (`cli_behavior.rs`) now drives the orchestrator through the binary: the
  kill-switch of each command (dispatch/automerge/deploy/loop) refuses
  hermetically, covering the `main.rs` arms no library test reaches.

## Consequences

- The envelope's most important property — kill-switch refuses before any side
  effect — is pinned end-to-end, not only in unit tests of the pure core.
- The floor is a one-way gate: coverage can only ratchet up.
