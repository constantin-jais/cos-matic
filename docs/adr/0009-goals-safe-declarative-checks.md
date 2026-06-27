# ADR-0009 — Goals: safe declarative checks, hard-gate vs observability

- Status: accepted
- Date: 2026-06-27

## Context

A "goals" framework (from the predecessor governance tool) splits checks into
**hard gates** (blocking) and **observability** metrics (non-blocking). The
predecessor expressed a gate as a shell command (`validation = "! grep ..."`).

Porting that verbatim is wrong on two axes:

- **Security (axis 1):** a shell-command gate means any cloned `harness.toml`
  executes arbitrary commands — a remote-code-execution surface in a config file.
- **Scope:** running arbitrary commands re-implements CI / pre-commit, which is
  not a config compiler's job (and which adjacent tools delegate to lefthook).

## Decision

Goals are **safe, declarative checks the tool computes itself** over the
`ConfigTree`. **No shell, ever.** A goal declares a `kind` and a named `check`:

```toml
[[goals]]
kind = "hard_gate"        # blocks: nonzero exit, no files written
check = "no-dead-domains"

[[goals]]
kind = "hard_gate"
check = "max-content-lines"
max = 400                 # the content-filtering lesson, as a budget

[[goals]]
kind = "observability"    # reported, never blocks
check = "require-domains"
domains = ["security-baseline"]
```

Check registry (Phase 4):

- `no-dead-domains` — every domain is selected by at least one profile.
- `require-domains` — the named domains all exist (param `domains`).
- `max-content-lines` — the largest profile's merged content stays within `max`
  lines (param `max`); without `max`, it just reports the metric.

Semantics: a `hard_gate` whose check fails makes the run fail **before any file
is written**. An `observability` goal is reported and never blocks. `aom goals`
evaluates and prints every outcome without writing (a CI gate surface); `aom
generate` enforces the hard gates as part of compilation.

## Consequences

- The manifest stays pure data: no executable surface, deterministic, auditable —
  consistent with the project's wedge.
- Adding a check = one entry in the check registry plus a pure function over the
  `ConfigTree`. Shell-based or content-of-output checks are explicitly out of
  scope; output-shape concerns are covered at the config level (a profile's
  merged content is known before rendering).
- New error variants: `GoalsFailed` (lists the failed gates) and `UnknownCheck`.
