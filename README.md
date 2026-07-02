# Bolt-Cos-Matic

[![CI](https://github.com/constantin-jais/bolt-cos-matic/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/constantin-jais/bolt-cos-matic/actions/workflows/ci.yml)
[![Security](https://github.com/constantin-jais/bolt-cos-matic/actions/workflows/security.yml/badge.svg?branch=main)](https://github.com/constantin-jais/bolt-cos-matic/actions/workflows/security.yml)
[![Contracts](https://github.com/constantin-jais/bolt-cos-matic/actions/workflows/contracts.yml/badge.svg?branch=main)](https://github.com/constantin-jais/bolt-cos-matic/actions/workflows/contracts.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

**Layer:** Bolt — Orchestration  
**Role:** deterministic intent-to-execution brain  
**Mission:** turn high-level operational intent into safe, inspectable plans across agents, tools, and repositories.

## Naming conventions

- Repository / package: `bolt-cos-matic`.
- CLI binary: `bolt-cosmatic`.
- Rust library crate: `bolt_cos_matic`.
- Engine environment variables: `BOLT_COSMATIC_*`.
- Public sandbox guard: `BOLT_HARNESS_SANDBOX` in [`bolt-harness`](https://github.com/constantin-jais/bolt-harness).

Legacy `aom` / `AOM_*` names are obsolete and should not be reintroduced outside
historical ADR context.

## Quickstart

```sh
git clone https://github.com/constantin-jais/bolt-cos-matic.git
cd bolt-cos-matic
cargo test --workspace --all-features
cargo run -q --bin bolt-cosmatic -- goals --manifest harness.toml
cargo run -q --bin bolt-cosmatic -- generate --check --manifest harness.toml
```

Use [`bolt-harness`](https://github.com/constantin-jais/bolt-harness) for public
sandbox proof runs. The engine repository keeps only read-only smoke workflows.

---

## Stack role

- **Layer:** Bolt — Orchestration.
- **Role:** deterministic intent-to-execution brain.
- **Mission:** turn high-level operational intent into safe, inspectable plans across agents, tools, and repositories.
- **Maturity:** `usable`.
- **Scale-ready:** no — useful for local harness workflows, but trust gates must harden before broader runtime expansion.
- **Current increment:** P4 orchestration integrated.
- **Learning value:** deterministic planning, refusals, safe writes, policy gates, and auditable agentic work.
- **Next quality step:** keep planning/refusal/evidence gates hardened before runtime expansion.

See the ecosystem cockpit in [`constantin-jais/ecosystem/status.md`](https://github.com/constantin-jais/constantin-jais/blob/main/ecosystem/status.md).

## Dogfooding

This repository is part of the forge dogfooding loop: the ecosystem should use its own tools to make specs, maturity, contracts, releases, and product documentation observable.

Current visible evidence:

- CI and contract workflows exercise planning, refusal, and harness behavior;
- the README and ADRs expose maturity, safe-write guarantees, and known boundaries;
- local harness commands can be used to validate generated plans and goals.

Expected next evidence:

- publish more example outputs for plans, refusals, and evidence reports;
- make dogfooding command transcripts easier to reproduce from the quickstart.

Dogfooding claims should stay backed by visible commands, fixtures, CI workflows, generated reports, or linked docs.

## Contributing

See:

- [CONTRIBUTING.md](CONTRIBUTING.md) for contribution guidelines;
- [ROADMAP.md](ROADMAP.md) for current contribution priorities;
- [docs/versioning.md](docs/versioning.md) for maturity and release typology;
- [docs/secrets.md](docs/secrets.md) for credential rotation and storage;
- [docs/local-llm.md](docs/local-llm.md) for local LM Studio / Gemma smoke tests;
- [issue templates](.github/ISSUE_TEMPLATE/) for bugs, docs issues, fixture/example requests, and design discussions.

## Forge role

`bolt-cos-matic` is the Bolt coordination layer: it turns Rumble product intent into safe plans, policy-gated actions, and inspectable execution evidence. It composes Portal client-platform work, Wrench capabilities, and Gear substrates instead of exposing a user product UI.

## Boundary

It must not own product UX, client-platform/design-system semantics, canonical extraction runtime, persistent memory, artifact registries, or model hosting. Product needs should remain in Rumble; client primitives belong to Portal; reusable inspection belongs to Wrench; extraction/runtime substrate and durable infrastructure belong to Gear.

## Purpose

`bolt-cos-matic` is the central orchestrator of the ecosystem. It receives goals, applies policy gates, selects tools, sequences actions, and records decisions.

It transforms:

> what should be done → how it will be executed safely

## Owns

- Agentic orchestration and delegation.
- Config compilation for coding agents and operational harnesses.
- Safe-write, drift detection, gates, incidents, and execution evidence.
- Coordination of Portal client-platform work, Wrench tools, and Gear substrates.

## Does Not Own

- Product UX: belongs to Rumble.
- Canonical extraction/parsing runtime: belongs to Gear Loader.
- Inspection/evidence over extraction outputs: belongs to Wrench.
- Persistent memory, artifact storage, registry, or runtime substrate: belongs to Gear.
- Generic chat UI or model hosting.

## Allowed Dependencies

- Calls **Wrench** tools for inspection, validation, and evidence.
- Calls **Gear Loader** for canonical extraction when a runtime/source pipeline is needed.
- Reads/writes context through **Gear** primitives.
- Serves **Rumble** products that need orchestration.

## Product Vision Challenge

`bolt-cos-matic` must stay a deterministic orchestrator, not become an all-purpose agent product. Its value is trust: explicit plans, reversible writes, gates, and auditable outcomes.

## Daily Use

Install the CLI locally:

```sh
cargo install --path crates/cli
```

Then use `bolt-cosmatic` as the session harness:

```sh
bolt-cosmatic generate --check --manifest harness.toml
bolt-cosmatic goals --manifest harness.toml
```

Local-only stack validation helpers are available under `stack`:

```sh
bolt-cosmatic stack project-status --root .
bolt-cosmatic stack detect --root . --json
bolt-cosmatic stack scorecard --root .
bolt-cosmatic stack dependency-audit --root .
bolt-cosmatic stack local-smoke --root . --cmd "cargo test --workspace --all-targets"
bolt-cosmatic stack db_security_check --root . --json
bolt-cosmatic stack adr_generate --title "Decision" --accepted-decision-ref "decision-log#id" --context "..." --decision "..." --consequence "..." --reversibility "..."
bolt-cosmatic stack deploy_dry_run --root . --cmd "cargo test --workspace --all-targets" --json
```

Planning-only handoffs can also consume Wrench EvidenceReport files, Gear manifests, or signed human approval contracts as ephemeral refs. Human approvals are verified through a local approval key registry lookup by `public_key_ref`; unknown, revoked, or expired keys refuse the checkpoint. Bolt reads only status/hash/provenance metadata to derive `evidence_refs[]`/`artifact_refs[]` and gate status; it does not store report, artifact, or approval bodies in its plan:

```sh
bolt-cosmatic handoff plan handoff.json --dry-run --json \
  --evidence-report wrench-portal-evidence.json

bolt-cosmatic handoff plan handoff.json --dry-run --json \
  --evidence-manifest gear-wrench-evidence-manifest.json

bolt-cosmatic handoff plan handoff.json --dry-run --json \
  --human-approval human-approval.json \
  --approval-key-registry approval-key-registry.json
```

They are designed for scorecards, gates, fixtures, ADR drafts, and dry-runs only: no provisioning, no provider activation, no remote secrets, and no automatic ADR acceptance.

This repository dogfoods `harness.toml` to generate `AGENTS.md`, `CLAUDE.md`,
and Cursor rules from one source of truth. See
[`docs/codex-routine.md`](docs/codex-routine.md) for the full Codex session
workflow.
