# ADR-0024 — A stub fixer to validate the loop's plumbing without an LLM

## Status

Accepted (2026-06-28).

## Context

A full live run of the loop needs the dispatch stage to invoke headless Claude,
which needs an `ANTHROPIC_API_KEY`. But the genuinely novel, risky part of the
loop is the autonomous plumbing — publish -> gate-that-waits -> merge -> deploy —
not the LLM. Gating a first live validation on having an Anthropic key conflates
the two.

## Decision

A `StubFixer` (selected with `AOM_FIXER=stub`) makes one deterministic, harmless
change (appends a line to `SANDBOX_FIXES.md`) and commits it — producing a real
branch that flows through the rest of the loop, with no LLM and no key.

- It runs inside dispatch's envelope (kill-switch, scope-fence, single attempt)
  and the same worktree isolation + commit path as the real fixer.
- The change never breaks the build, so the branch sails through CI and the
  green-only gate, exercising publish/automerge/deploy end to end.
- The `orchestrator-loop.yml` workflow gains a `fixer` input (claude | stub); in
  stub mode it skips installing Claude and needs no `ANTHROPIC_API_KEY`.

## Consequences

- The autonomous plumbing can be proven live on a sandbox today; the real
  `ClaudeFixer` drops in later by flipping the input — no rewrite.
- The stub is a permanent testing affordance: it isolates loop-mechanics failures
  from fixer-quality failures, and runs without spending Anthropic tokens.
