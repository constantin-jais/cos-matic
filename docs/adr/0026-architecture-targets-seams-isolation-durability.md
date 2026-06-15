# ADR-0026 — Architecture targets: Forge and FixerRuntime seams, isolation, durability

## Status

Proposed (2026-06-28).

## Context

The orchestrator's core loop (dispatch → publish → automerge → deploy) runs
across three critical boundaries: the external forge (GitHub), fixer execution
(an agent with constrained tools), and durable state. The first end-to-end live
run (a throwaway sandbox, 12 iterations) exposed integration bugs at each
boundary — about a dozen across ten fix commits, plus two credential
corrections on the GitHub side — validating the thesis that _a 0%-live-tested
autonomous system is a skeleton until it runs for real_. The bugs concentrated
in:

- **Forge coupling.** Roughly half traced straight to shelling out to the `gh`
  CLI: `github.token` cannot read the `statusCheckRollup` GraphQL; exit codes
  were read as status; PR-existence was checked via the wrong API.
- **Isolation.** The fixer ran with constrained-but-real Bash inside an
  ephemeral runner; the allow-list + `dontAsk` hardening (ADR:
  bounded-fixer-bash-hardening) proved the pattern but left process-level
  containment as a future step.
- **State.** Audit is ephemeral JSONL; no recovery, no replay, no structured
  event log.

Patterns that held up: traits for offline testing (`Forge`, `Fixer`, `Stages`,
`Deployer`, `Smoke`), zero-PII audit, and a trigger-agnostic loop core. This ADR
names the seams to harden and the targets for isolation, state, and multi-forge.

## Decision

Six architectural targets, grouped by maturity.

### 1. Forge trait — consolidate the orchestrator on octocrab (the refactor-first target)

The incident→issue layer already uses octocrab behind a `Forge` trait (ADR:
github-via-octocrab). But the orchestrator's **gate, publish, and merge still
shell out to `gh`** (five call sites in `crates/orchestrator`), and that
coupling caused roughly half the live-debug bugs.

**Decision:** extend the `Forge` trait to cover every operation the loop needs —
open issue, push branch, open PR, read check status, merge — and consolidate all
of it on octocrab, deleting the remaining `gh`-subprocess call sites. Typed API
calls remove the output-parsing and exit-code-as-status traps; `FakeForge` keeps
the decision logic offline-testable. **This refactor comes before the onboarding
work**: it pays the live-debug debt and is the seam that makes multi-forge (§6)
additive rather than a rewrite.

### 2. FixerRuntime / Sandbox trait — isolation boundary (NOT YET IMPLEMENTED)

The fixer is a trait (`Fixer`, with `ClaudeFixer` + `FakeFixer`), but its
_execution environment_ is not abstracted. Current: constrained Bash inside a CI
runner. Target: a `FixerRuntime` trait with pluggable isolation.

- **V1 — gVisor (`runsc`).** Syscall interception, no nested virtualization,
  broad host coverage; the fixer runs rootless with no host write, bounded
  network, no repo-external files. ~100 ms startup per attempt.
- **Target — Firecracker microVM.** True VM isolation. A dedicated
  fixer-service runs microVMs on a KVM host; a trait swap, not a call-site
  rewrite.
- **Why both.** The allow-list (ADR: bounded-fixer-bash-hardening) is
  defense-in-depth, not containment: a hostile `cargo` build script still runs
  code. gVisor closes most of that gap now; Firecracker closes the rest when the
  infra is worth standing up.

### 3. Typed-TOML policy (NOT YET IMPLEMENTED)

Policy is **configuration, not a rule engine**. No `Policy` struct exists yet;
the target is a serde struct backed by TOML:

```toml
[autonomy]
level = "L2"  # L0 compile-only, L1 dispatch, L2 gated loop, L3 trusted

[policy]
require_approval_before_merge = true
max_merges_per_window = 10
allowed_branches = "^(main|release-.*)$"

[scope]
allowed_repos = ["owner/repo"]
```

Not OPA/Rego: a rule engine is gold-plating at this scale. A typed struct is
simpler to audit, binds to the safety envelope (ADR:
workspace-and-orchestrator-charter), and versions alongside the code. Every
autonomy decision then logs its level and any approval/scope check — policy as
auditable fact, not implicit in code.

### 4. Run-state durability: SQLite, not Temporal (NOT YET IMPLEMENTED)

State evolves from ephemeral JSONL audit → a **SQLite-backed run-state**.

- **Why SQLite, not Temporal.** The self-hostable single-binary ethos. Temporal
  is a full external service with its own runbooks; SQLite is embedded,
  zero-config, and the right tool for local-first durable state.
- **Schema (future):** `runs(id, trigger, autonomy_level, outcome, ts)`,
  `events(run_id, stage, action, status, ts)`. Zero PII. Enables resume, replay,
  and observability.
- **Timing.** The JSONL audit is enough for the sandbox run; SQLite lands once
  the loop stabilizes and replay semantics are validated.

### 5. Loop core — pure and trigger-agnostic (already proven)

The loop core (`run_until_done` over the `Stages` trait) has zero knowledge of
CI (ADR: end-to-end-loop). The stages (dispatch → publish → automerge → deploy)
compose with short-circuit logic proven offline via `FakeStages`; `cosmatic loop`
takes incident/repo/branch as CLI args, with no `github.event` parsing.

**Decision (graven as a constraint):** the core must never leak "CI". That is
what makes CI-workflow → long-lived daemon a _graduation, not a rewrite_ — swap
the trigger (workflow_dispatch → cron/systemd/k8s), keep the core; durable
run-state (§4) makes resumption auditable.

### 6. Multi-forge by design (GitHub-only today)

Once §1 lands, GitLab (or Gitea) is an additive `GitLabForge` impl behind the
same trait, not a rewrite. The gate currently reads GitHub check-runs only; a
multi-forge gate (waiting on checks across forges) is a later ADR. Design for
multi-forge; implement GitHub only until a second forge is actually needed.

## Consequences

- **Clear seams, low coupling.** Forge, FixerRuntime, and Policy are
  traits/configs; orchestration is decoupled from forge choice, isolation tech,
  and policy syntax. Evolution does not ripple.
- **Sequencing.** §1 (Forge/octocrab) is the first build — it pays debt and
  unblocks §6. Isolation (§2), policy (§3), and durability (§4) follow; the
  daemon graduation (§5) is last.
- **Honest limitations.** Since ADR-0027, GitHub operations run through the
  `Forge`/octocrab seam; only `git push` remains a subprocess. The gate wait is
  implemented for GitHub check-runs, not yet as a multi-forge abstraction. Deploy
  runs caller-provided commands (the canary→rollback orchestration is built; the
  commands are yours). gVisor overhead is ~100 ms; the current allow-listed Bash
  is defense-in-depth, not containment.
- **Graduations, not rewrites.** Daemon, multi-forge, Firecracker, and SQLite
  are each a bounded, incremental ADR, because the seams above absorb them.

## Next steps (separate ADRs)

- Forge trait completion + octocrab consolidation (kill the `gh` call sites).
- FixerRuntime trait + gVisor V1.
- Policy struct + audit integration (autonomy levels L0–L3 as configuration).
- SQLite run-state (durability + replay).
- Multi-forge gate; daemon graduation.
