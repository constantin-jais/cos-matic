# Agent-O-Matic

A deterministic, agent-agnostic system for **trustworthy autonomous code-ops**:
one declarative manifest compiles to many AI-agent configurations (safe-write,
drift-aware), and an autonomous CI/CD loop handles incidents end-to-end
(incident → issue → bounded fix → gate-and-merge → deploy-or-rollback) — all
under a reversible safety envelope **you** own.

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Rust 1.95+](https://img.shields.io/badge/Rust-1.95%2B-orange.svg)](https://www.rust-lang.org)
[![CI](https://github.com/constantin-jais/Agent-O-Matic/actions/workflows/ci.yml/badge.svg)](https://github.com/constantin-jais/Agent-O-Matic/actions/workflows/ci.yml)

**Status — `v0`: compiler proven, orchestrator live-tested.** Built as a
clean-room learning artifact: every non-obvious decision is an ADR in
[`docs/adr/`](docs/adr/), and tests are the executable spec. The north-star —
_autonomy you would actually enable, because safety (L0→L3), legibility (the ADR
archive), and forkability are first-class_ — is set in
[ADR-0025](docs/adr/0025-north-star-trustworthy-autonomy.md); the positioning in
[ADR-0001](docs/adr/0001-positioning-and-why-build.md).

## What it does: two halves, one whole

### The compiler

You keep one source of truth: a `harness.toml` manifest declaring reusable
instruction **domains**, **profiles** (named subsets), and **targets** (per-agent
outputs); Markdown content lives in its own files. `aom generate` compiles that
source into each agent's native config — deterministically, with **safe-write**
(it never clobbers your hand-edits) and **drift detection** (a CI gate that keeps
outputs in sync with source).

```toml
# harness.toml
[package]
name = "my-project"

[[domains]]
name = "code-style"
priority = 8
content_file = "domains/code-style.md"

[[profiles]]
name = "default"
domains = ["code-style"]

[[targets]]
name = "agents-md"
adapter = "universal"
output_file = "AGENTS.md"
profile = "default"
```

Three adapters today: **universal** ([`AGENTS.md`](https://agents.md/)),
**Claude** (`CLAUDE.md` + Tier-2 subagents/skills/hooks, ADR-0013), and
**Cursor** (per-domain `.mdc` rules with glob activation).

### The orchestrator

An autonomous incident-response loop, behind a hard, reversible safety envelope:

```
incident (reported / detected)
  → issue       (idempotent GitHub issue, fingerprinted)
  → dispatch    (bounded fix in an isolated worktree; the agent edits, never merges)
  → publish     (push the branch + open the PR)
  → automerge   (gate: merge on green evidence only — never red/unknown)
  → deploy      (canary → smoke → promote-or-rollback)
  (stop at the first stage that does not advance; retry next iteration if the issue is still open)
```

Each stage is independently callable (`aom incident open`, `aom dispatch --issue
8`, …) and composes via `aom loop` into a full end-to-end run. The envelope —
kill-switch, scope-fence (repo allowlist), circuit-breaker, zero-PII audit,
reversible by design — is binding at every link.

## Autonomy levels: earn trust step by step

Four levels, so you start safe and scale trust as confidence grows. The
discriminator is not raw autonomy — it is autonomy you can _audit, bound, and
self-host_ (ADR-0025).

| Level  | What runs                                       | Autonomy                     | Human gate                  | Status                          |
| ------ | ----------------------------------------------- | ---------------------------- | --------------------------- | ------------------------------- |
| **L0** | Config compilation only                         | None                         | n/a                         | Shipped                         |
| **L1** | Bounded fix in an isolated worktree             | Proposes (never merges)      | You review + merge          | Shipped                         |
| **L2** | Full loop (dispatch → publish → merge → deploy) | Proposes + lands             | Approve-before-merge        | Proven live (stub fixer)        |
| **L3** | Full auto, merge gate automatic (green = go)    | Full, under policy + sandbox | Circuit-breaker + kill only | Loop proven; isolation = target |

**Entry point: L0 is your on-ramp.** Compile a `harness.toml`, hand-review the
output, feel the safe-write and drift detection. Add the orchestrator when ready.
Each level is an _additive_ envelope: L1 → L2 → L3 adds evidence checks and
removes human gates — not a rewrite. The decision logic is proven offline via
traits (`Fixer`, `Gate`, `Forge`, `Stages`); live runs validate the boundary.

## Quick start: L0 (config compilation)

```sh
cargo build --release          # → target/release/aom
```

```toml
# harness.toml
[package]
name = "my-project"
version = "0.1.0"

[[domains]]
name = "core-values"
priority = 10
content = "Be explicit. Explain the why."

[[profiles]]
name = "default"
domains = ["core-values"]

[[targets]]
name = "agents-md"
adapter = "universal"
output_file = "AGENTS.md"
profile = "default"
```

```sh
aom generate           # compile the source into each target's native config
aom generate --check   # CI gate: non-zero exit if any output drifted from source
aom generate --force   # overwrite even outputs you hand-edited since the last write
```

`-m, --manifest <path>` points at a manifest other than `./harness.toml`.

**Hand-edit without fear.** After generation, edit the output (e.g. `AGENTS.md`)
freely. The next `aom generate` will not overwrite diverged content (it refuses
with a clear message unless you pass `--force`), records what it wrote in
`.harness/lock.toml` (committed; never hand-edited), and stays deterministic.

```sh
aom library list           # built-in domains
aom library show <name>    # a domain's content
aom goals -m harness.toml  # evaluate declared goals (hard-gate vs observability)
```

## Architecture

### Compiler pipeline

```
harness.toml + domains/*.md            (one source of truth)
   │  aom generate
   ▼
parse → resolve → ir → merge(priority) → render → safe-write → audit
                                          ▲
                           .harness/lock.toml  (guards hand-edited outputs)
   ▼
AGENTS.md · CLAUDE.md (+ Tier-2) · .cursor/rules/*.mdc
```

Deterministic (same source → same output), safe-write (never silently clobbers
edits), drift-detectable (`--check` as a CI gate), priority-merged (explicit,
auditable composition), with an embedded content library (`library://` + curated
builtins, ADR-0008).

### Orchestrator loop

```
┌──────────────────────────────────────────────────────────┐
│ Safety envelope (binding from L1 onward)                  │
│  • Kill-switch (AOM_*_DISABLED)   • Scope-fence (allowlist)│
│  • Circuit-breaker (max attempts/merges/iterations)       │
│  • Zero-PII audit (JSONL)         • Reversible by design   │
└──────────────────────────────────────────────────────────┘
   incident → forge(issue, idempotent) → dispatch(fixer, edit-only, worktree)
   → publish(push + PR) → automerge(verdict: Green|Red|Unknown; only Green merges, fail-closed)
   → deploy(canary → smoke → promote | rollback) → [retry if issue still open]
```

Each stage implements the `Stages` trait, so the loop logic is proven offline via
`FakeStages`; only the live boundary (`RealStages`, the GitHub forge, the fixer,
`gh`/git) touches the network. `aom loop` short-circuits at the first stage that
does not advance — no fix branch → stop; gate not green → stop; smoke failed →
auto-rollback and stop (ADR-0017).

### Safety envelope (binding at L1+)

1. **Hard, evidence-backed gates.** Nothing merges or deploys without attached
   green proof; the gate refuses what it cannot verify (fail-closed, ADR-0015).
   It _polls_ CI checks until they register and settle, then decides (ADR-0020).
2. **Reversible deploys.** Canary → smoke → promote-or-auto-rollback; a failed
   smoke rolls back immediately (ADR-0016).
3. **Circuit-breaker.** Per-stage and global budgets (fix attempts, merges per
   window, loop iterations) stop runaway loops (ADR-0022).
4. **Zero-PII audit.** Every action is journaled to JSONL (incident type, issue,
   action, outcome, timestamp) — no diffs, no paths, no tokens, no usernames.
5. **Scope-fence + kill-switch.** The loop is confined to a repo allowlist and
   never touches infra credentials; one env var disables it globally.

**Run as a scoped CI bot, not a human token.** The loop runs under
`github-actions[bot]`, narrowed by a `permissions:` block; live runs use a
fine-grained PAT scoped to one sandbox repo, revoked by deleting a secret — no
account-level risk (ADR-0019).

## Tech stack, with rationale

| Component               | Choice                                     | Why                                                                                                                                                                                                                                                                            |
| ----------------------- | ------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| **Language**            | Rust, edition 2024                         | Memory safety without a GC; a single self-contained binary (decisive for a tool meant to be forked + self-hosted); the type system enforces the safe-write state machine at compile time. (ADR-0002)                                                                           |
| **Async runtime**       | `tokio` + `async-trait`                    | Required by `octocrab`. Confined to the forge boundary; the compiler stays synchronous.                                                                                                                                                                                        |
| **GitHub API**          | `octocrab` (typed client)                  | Used today for the incident→issue layer (ADR-0012). The orchestrator's gate/publish/merge still shell out to `gh` (5 sites) — consolidating them onto `octocrab` behind the `Forge` trait is the **next refactor**; that subprocess coupling caused ~half the live-debug bugs. |
| **Diagnostics**         | `miette`                                   | Rich, pointed errors from day one — errors should teach. (ADR-0005)                                                                                                                                                                                                            |
| **Config & policy**     | `serde` + `toml`                           | A typed manifest _and_ typed policy — declarative, auditable, no separate rule engine. (ADR-0026)                                                                                                                                                                              |
| **CLI**                 | `clap`                                     | Standard derive-macro CLI.                                                                                                                                                                                                                                                     |
| **Wizard prompts**      | `inquire` (planned)                        | Prompts for the `aom init` onboarding wizard (L0–L3 presets).                                                                                                                                                                                                                  |
| **Content integrity**   | `blake3`                                   | Fast content fingerprints for the lockfile (drift detection) and incident idempotency.                                                                                                                                                                                         |
| **Forge seam**          | `Forge` trait (`GithubForge`, `FakeForge`) | GitHub-first; designed so GitLab/Gitea are additive impls (design-for-multi-forge, implement-GitHub-only). (ADR-0026 §1, §6)                                                                                                                                                   |
| **Fixer isolation**     | `Fixer` trait → `FixerRuntime` (target)    | Today: headless Claude with an allow-list (`Edit Write Read Grep Glob Bash(cargo *)`), in an ephemeral runner. Target: gVisor (V1) → Firecracker microVM. (ADR-0023, ADR-0026 §2)                                                                                              |
| **Policy**              | Typed TOML (`[autonomy]`, `[policy]`)      | Declarative, versionable, auditable. No OPA/Rego — a rule engine is overkill here. (ADR-0026 §3)                                                                                                                                                                               |
| **Durability (future)** | SQLite run-state                           | Proto: the zero-PII JSONL audit. Target: a SQLite run-ledger for resume + replay. **Not** Temporal — an external service fights the self-hostable ethos. (ADR-0026 §4)                                                                                                         |

**Why Rust over Go/TS:** the type system lets the compiler enforce the safe-write
state machine and the determinism guarantees at compile time, and a single
self-contained binary is what makes the tool forkable and self-hostable with no
runtime to install (ADR-0002).

## Why these choices: the decision log

Every non-obvious choice is an ADR, in `Status / Context / Decision /
Consequences` form, so the repo doubles as teaching material. The load-bearing ones:

| What                     | ADR                                                                      |
| ------------------------ | ------------------------------------------------------------------------ |
| Positioning / why build  | [0001](docs/adr/0001-positioning-and-why-build.md)                       |
| Language: Rust           | [0002](docs/adr/0002-language-rust.md)                                   |
| Safe-write lockfile      | [0004](docs/adr/0004-safe-write-sentinel-lockfile.md)                    |
| Drift as a CI gate       | [0010](docs/adr/0010-drift-as-ci-gate.md)                                |
| Workspace + orchestrator | [0011](docs/adr/0011-workspace-and-orchestrator-charter.md)              |
| GitHub via octocrab      | [0012](docs/adr/0012-github-via-octocrab.md)                             |
| Bounded dispatch         | [0014](docs/adr/0014-dispatch-bounded-fixer.md)                          |
| Autonomous merge         | [0015](docs/adr/0015-autonomous-merge.md)                                |
| Deploy + rollback        | [0016](docs/adr/0016-deploy-canary-smoke-rollback.md)                    |
| End-to-end loop          | [0017](docs/adr/0017-end-to-end-loop.md)                                 |
| Scoped CI bot            | [0019](docs/adr/0019-operate-loop-as-scoped-ci-bot.md)                   |
| Merge gate waits         | [0020](docs/adr/0020-merge-gate-waits-for-checks.md)                     |
| Bounded-fixer hardening  | [0023](docs/adr/0023-bounded-fixer-bash-hardening.md)                    |
| **North-star**           | [0025](docs/adr/0025-north-star-trustworthy-autonomy.md)                 |
| **Architecture targets** | [0026](docs/adr/0026-architecture-targets-seams-isolation-durability.md) |

**26 ADRs** in [`docs/adr/`](docs/adr/) — the full reasoning, cross-linked.

## Forking & self-hosting

Built to be forked: no external services, no cloud registry, no vendor lock-in.

- **Compiler:** `cargo build --release` → one binary. Ship via `cargo install`,
  a container image, or git.
- **Orchestrator:** the loop runs in CI (`.github/workflows/orchestrator-loop.yml`)
  under a scoped bot identity; adapt the workflow to your CI.
- **Policy:** declarative TOML in your repo — no external rule engine.
- **Audit:** zero-PII JSONL you keep.

**Swap the fixer runtime** (the `Fixer` trait is built for it): V1 (today)
headless Claude in the runner; V2 gVisor on the runner (syscall isolation, no
nested virt); V3 Firecracker microVM on a KVM host you own. Swapping is a config
change, not a rewrite (ADR-0026 §2).

**Multi-forge** is designed, not built: the `Forge` trait is the seam; GitHub is
the first full impl, GitLab/Gitea are additive once the orchestrator finishes
consolidating onto it (ADR-0026 §1, §6).

## Status & limitations

**Proven.** The compiler (safe-write, drift, feature-gating) ships and is
dogfooded on this repo's own config. The full autonomous loop ran live
end-to-end as `github-actions[bot]` on a throwaway sandbox — it merged its own PR
and landed the change on `main`. Getting there took a 12-run live debug that
exposed **about a dozen integration bugs** (ten fix commits, plus two GitHub-side
credential corrections): git author identity on a fresh runner, branch collision
on retry, push not triggering CI, PAT scopes, non-fast-forward stale branch, `gh`
exit-code-vs-JSON, PR-create idempotency, a checks-not-yet-registered race, the
two-token model, the "Actions may create PRs" setting, and `github.token` being
unable to read `statusCheckRollup` (→ the REST check-runs API). **None were
catchable by the offline Fake-based tests** — the project's own thesis (_a
0%-live-tested autonomous system is a skeleton until it runs for real_),
confirmed.

**Known limitations** (deferred by design, not bugs):

| Limitation                               | Why / status                                                                                                                                                                 |
| ---------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Orchestrator still shells out to `gh`    | Gate/publish/merge use 5 `gh`-subprocess calls; consolidating onto `octocrab` behind the `Forge` trait is the next refactor (ADR-0026 §1).                                   |
| Deploy runs _your_ commands              | The canary→smoke→promote/rollback orchestration is built (`Deployer` + `Smoke` traits); the actual deploy/smoke commands are caller-provided (the sandbox ran no-op `true`). |
| Fixer is process-isolated, not sandboxed | V1 runs the fixer in the runner with an allow-list (defense-in-depth, not containment). gVisor (V2) / Firecracker (V3) are the target (ADR-0026 §2).                         |
| Multi-forge designed, not built          | `Forge` trait exists; only `GithubForge` is complete (ADR-0026 §6).                                                                                                          |
| Run-state is ephemeral (JSONL audit)     | No persistent ledger yet; resume/replay are manual. SQLite run-state is the target (ADR-0026 §4).                                                                            |
| Daemon mode is future                    | The loop runs on-demand in CI; the core is trigger-agnostic, so a durable daemon is a graduation, not a rewrite (ADR-0026 §5).                                               |

The gaps are known-deferred; the seams are designed to fill them without rewrites.

## Build & test

```sh
cargo build --release
cargo test --workspace                                  # offline; no live GitHub calls
cargo clippy --workspace --all-targets -- -D warnings   # zero-warning gate
cargo fmt --all --check
aom generate --check                                    # dogfoods the compiler on this repo
```

## Contributing

A learning-first, clean-room project — the reasoning is part of the artifact.
Architectural change? Write an ADR first. Non-trivial logic? Bring tests; they
are the spec. See [`CONTRIBUTING.md`](CONTRIBUTING.md) and start with the
[ADR archive](docs/adr/).

## Further reading

- **[Orchestrator runbook](docs/orchestrator-runbook.md)** — set up + run the loop in CI.
- **[ADR archive](docs/adr/)** — 26 decisions, the full reasoning.
- **[Examples](examples/)** — a minimal `harness.toml`.

## License

MIT — see [LICENSE](LICENSE).
