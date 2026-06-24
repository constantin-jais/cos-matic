# Cos-Matic

[![CI](https://github.com/constantin-jais/cos-matic/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/constantin-jais/cos-matic/actions/workflows/ci.yml)
[![Security](https://github.com/constantin-jais/cos-matic/actions/workflows/security.yml/badge.svg?branch=main)](https://github.com/constantin-jais/cos-matic/actions/workflows/security.yml)
[![Contracts](https://github.com/constantin-jais/cos-matic/actions/workflows/contracts.yml/badge.svg?branch=main)](https://github.com/constantin-jais/cos-matic/actions/workflows/contracts.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

**Layer:** Bolt — Orchestration  
**Role:** deterministic intent-to-execution brain  
**Mission:** turn high-level operational intent into safe, inspectable plans across agents, tools, and repositories.

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

## Forge role

`cos-matic` is the Bolt coordination layer: it turns Rumble product intent into safe plans, policy-gated actions, and inspectable execution evidence. It composes Wrench capabilities and Gear substrates instead of exposing a user product UI.

## Boundary

It must not own product UX, raw extraction, persistent memory, artifact registries, or model hosting. Product needs should remain in Rumble; reusable inspection belongs to Wrench; durable infrastructure belongs to Gear.

## Purpose

`cos-matic` is the central orchestrator of the ecosystem. It receives goals, applies policy gates, selects tools, sequences actions, and records decisions.

It transforms:

> what should be done → how it will be executed safely

## Owns

- Agentic orchestration and delegation.
- Config compilation for coding agents and operational harnesses.
- Safe-write, drift detection, gates, incidents, and execution evidence.
- Coordination of Wrench tools and Gear substrates.

## Does Not Own

- Product UX: belongs to Rumble.
- Raw extraction/parsing: belongs to Wrench.
- Persistent memory, artifact storage, registry, or runtime substrate: belongs to Gear.
- Generic chat UI or model hosting.

## Allowed Dependencies

- Calls **Wrench** tools for extraction, inspection, validation, and evidence.
- Reads/writes context through **Gear** primitives.
- Serves **Rumble** products that need orchestration.

## Product Vision Challenge

`cos-matic` must stay a deterministic orchestrator, not become an all-purpose agent product. Its value is trust: explicit plans, reversible writes, gates, and auditable outcomes.

## Daily Use

Install the CLI locally:

```sh
cargo install --path crates/cli
```

Then use `cosmatic` as the session harness:

```sh
cosmatic generate --check --manifest harness.toml
cosmatic goals --manifest harness.toml
```

This repository dogfoods `harness.toml` to generate `AGENTS.md`, `CLAUDE.md`,
and Cursor rules from one source of truth. See
[`docs/codex-routine.md`](docs/codex-routine.md) for the full Codex session
workflow.
