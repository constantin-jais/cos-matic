# ADR-0019 — Operate the autonomous loop as a scoped CI bot

## Status

Accepted (2026-06-28).

## Context

The orchestrator's outward actions (open/merge PRs, create issues, deploy) are
designed to run unattended behind the safety envelope. Validating them by handing
an interactive session a human's full-privilege `gh auth token` is the wrong
posture: over-privileged (the whole account, not one repo), mis-attributed (the
audit names a person, not a bot — defeating the zero-PII audit's intent), and not
finely revocable.

## Decision

The loop runs as a scoped service identity in CI
(`.github/workflows/orchestrator-loop.yml`):

- **Identity** = the ephemeral `github-actions[bot]`, narrowed by a `permissions:`
  block to contents+issues+pull_requests. That block IS the fine-grained scope —
  for the read-only dry-run there is no PAT at all.
- **Trigger** = `workflow_dispatch` only. The autonomous loop never fires on push.
- **Safe by default** = dry-run; the live path is fenced to a repo explicitly
  flagged `BOLT_HARNESS_SANDBOX=true`, and uses a fine-grained PAT scoped to the sandbox
  (needed only so the bot's push triggers CI, so the gate can go green).
- **Bash containment** = the headless fixer's `--allowedTools Bash` runs inside
  the throwaway runner, not a developer's machine.

## Consequences

- Autonomous actions are scoped, attributed to a bot, and revocable without
  touching anyone's personal access — the audit trail finally matches the design.
- Validation happens in the loop's intended unattended mode, not interactive
  theatre. The runbook (docs/orchestrator-runbook.md) documents setup and the
  honest limitations (the gate does not yet wait for pending checks).
