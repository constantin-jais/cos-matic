# ADR-0023 — Harden the bounded fixer: no arbitrary Bash

## Status

Accepted (2026-06-28).

## Context

The dispatch fixer ran headless Claude with `--allowedTools Edit Write Bash`. Bare
`Bash` is arbitrary command execution with the caller's local privileges — the
single sharpest risk in the system (the worktree isolates files, not the process:
the agent could `curl`, `rm`, read `~/.ssh`, reach the network). And the fixer was
expected to commit through that same Bash, leaving the commit implicit.

## Decision

- **Allow-list, not arbitrary Bash.** The fixer gets `Edit`, `Write`, `Read`,
  `Grep`, `Glob`, and `Bash(cargo *)` — it can edit, search, and run `cargo` to
  verify, nothing else. No git, no network, no `rm`, no arbitrary shell.
- **Fail closed.** `--permission-mode dontAsk` auto-denies anything off the list,
  so an unmatched tool fails immediately instead of prompting (which would hang a
  headless run).
- **Dispatch commits, not the fixer.** The fixer only edits; dispatch then commits
  its work, so the branch is publishable without granting the fixer git. An empty
  diff is an explicit failure ("no changes produced"), not a silent empty PR.

## Consequences

- The most dangerous capability is contained to `cargo` + file edits in a
  throwaway worktree — the blast radius of a misbehaving or prompt-injected fixer
  is bounded.
- Defense-in-depth, not a sandbox: a hostile `cargo` build script can still run
  code. Running the fixer inside the ephemeral CI runner (ADR: operate-loop-as-scoped-ci-bot)
  remains the stronger containment; a process-level sandbox/timeout is a further step.
