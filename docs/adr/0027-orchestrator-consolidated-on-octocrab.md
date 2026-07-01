# ADR-0027 — Orchestrator consolidated on octocrab

## Status

Accepted (2026-06-28). Delivers §1 of ADR:
architecture-targets-seams-isolation-durability.

## Context

The incident→issue layer already spoke to GitHub through a typed octocrab client
behind the `Forge` trait (ADR: github-via-octocrab), but the orchestrator's gate,
publish, and merge still shelled out to the `gh` CLI at five sites. Roughly half
the live-debug integration bugs traced straight to that coupling: a CI runner's
`github.token` cannot resolve the `statusCheckRollup` GraphQL path the CLI uses
for PR checks; `gh` exit codes were read as check status; PR existence was probed
with the wrong call. The `gh` seam was also untyped, version-fragile, and
invisible to the offline `Fake` tests.

## Decision

Extend the `Forge` trait to cover **every** GitHub operation the loop needs and
route all of them through octocrab, deleting the five `gh` call sites. `git push`
stays a subprocess — it is VCS, not forge API.

- **Four new `Forge` methods.** `list_check_runs` (the gate), `find_open_pr` and
  `create_pr` (publish), `merge_pr` (the merger). The check-run → bucket mapping
  the gate's old `--jq` filter did now lives in Rust (`check_bucket`), so the
  unit-tested `classify` is unchanged.
- **The 2-token model, as two clients.** The live debug proved a single token
  fails — a fine-grained PAT cannot read checks, a runner's `github.token` cannot
  open a PR (ADR: operate-loop-as-scoped-ci-bot). `GithubForge` now holds a write
  `client` (PR list/create/merge) and a `checks_client` (`BOLT_COSMATIC_CHECKS_TOKEN`,
  falling back to the write client) used only by `list_check_runs`. This replaces
  the old per-call `GH_TOKEN` env override.
- **Async all the way, blocked once.** octocrab is async; the gate, merger, and
  `Stages` were sync. Rather than thread a runtime into sync trait impls, the
  whole path is now async and the CLI blocks on it once at the boundary —
  exactly as the `incident` command already did. `Gate`/`Merger`/`Stages` are
  `#[async_trait(?Send)]` because the loop runs sequentially behind a single
  `block_on`, never spawned; `Forge` stays `Send`. The gate poll-loop swaps
  `thread::sleep` for `tokio::time::sleep`.
- **The forge-backed gate/merger become testable.** `ForgeGate` and `ForgeMerger`
  (replacing the old GitHub-CLI gate/merger) take a `&Forge`, so the in-memory
  `FakeForge` now exercises the no-PR, all-green, and any-fail paths offline —
  coverage the `gh`-subprocess gate never had (ADR: autonomous-merge, ADR:
  merge-gate-waits-for-checks).

## Consequences

- The traps that produced ~half the live-debug bugs are gone by construction:
  typed responses, no exit-code-as-status, no output parsing.
- `Forge` is now the real multi-forge seam: a `GitLabForge` is an additive impl,
  not a rewrite (ADR: architecture-targets-seams-isolation-durability §6).
- A new live boundary: the octocrab calls are unproven until they run for real
  (the `Fake` tests cannot exercise them). The change is gated on a live sandbox
  stub run, per the project's own thesis, before the consolidation is claimed
  done.
- `Octocrab` is constructed twice when `BOLT_COSMATIC_CHECKS_TOKEN` is set — negligible, and
  the cost of honestly modelling two distinct token scopes.
