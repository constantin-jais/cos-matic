# Harness Workspace Foundation (A0) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Convert the single-crate `agent-o-matic` repository into a Cargo workspace (`crates/aom` compiler library, `crates/cli` `aom` binary, `crates/orchestrator` stub) without any behavior change, keeping all 45 tests and the drift check green.

**Architecture:** A virtual workspace at the repo root. The compiler library (`agent_o_matic`) loses its CLI concern and its `clap` dependency; the `aom` binary moves to a thin `crates/cli` package that depends on the library; a new empty `crates/orchestrator` library is scaffolded as the home for the future agentic loop (A1+). This is a pure structural refactor — the existing test suite is the regression gate.

**Tech Stack:** Rust 2024, Cargo workspaces (`resolver = "3"`, `[workspace.dependencies]`, `[workspace.package]`), clap, miette, serde, blake3, toml, tempfile.

## Global Constraints

- Edition `2024`, `rust-version = "1.95"`, license `MIT` — copied verbatim into `[workspace.package]`.
- Zero clippy warnings under `RUSTFLAGS="-D warnings"`; `cargo fmt` clean.
- **No new runtime dependencies** in A0 — pure restructure. Any future dep is justified in an ADR.
- No machine-local absolute paths in any versioned file (use repo-relative / `import.meta`-style resolution; here, plain relative paths).
- The 45 existing tests (42 unit + 3 e2e) MUST stay green; `aom generate --check examples/minimal` MUST stay green (compiler non-regression).
- `crates/aom` stays clean-room: ADR-0001 remains valid for it; orchestration lives elsewhere.
- Determinism preserved (no logic touched).

## File Structure

```
Cargo.toml                                  # MODIFY: package → [workspace]
crates/
  aom/
    Cargo.toml                              # CREATE: lib agent_o_matic, no clap
    src/                                    # MOVED from ./src (minus main.rs, cli.rs)
      lib.rs                                # EDIT: drop `mod cli`, `run()`, clap use
      {config,render}/ + *.rs               # MOVED unchanged
    tests/e2e.rs                            # MOVED unchanged from ./tests/e2e.rs
  cli/
    Cargo.toml                              # CREATE: bin aom, deps agent_o_matic+clap+miette
    src/main.rs                             # CREATE: parse + dispatch (from old lib.rs run())
    src/cli.rs                              # MOVED unchanged from ./src/cli.rs
  orchestrator/
    Cargo.toml                              # CREATE: lib stub
    src/lib.rs                              # CREATE: doc + 1 smoke test
docs/adr/0006-workspace-and-orchestrator-charter.md   # CREATE
.github/workflows/ci.yml                    # MODIFY: --all → --workspace
examples/minimal/**                          # UNCHANGED (stays at repo root)
README.md, CONTRIBUTING.md, LICENSE          # UNCHANGED (stay at repo root)
```

---

### Task 1: Split the single crate into `crates/aom` (lib) + `crates/cli` (bin) under a workspace

Atomic refactor — a workspace move cannot be committed in green half-steps, so the whole split is one task ending green. The existing suite is the test.

**Files:**

- Modify: `Cargo.toml` (root → workspace)
- Create: `crates/aom/Cargo.toml`, `crates/cli/Cargo.toml`, `crates/cli/src/main.rs`
- Move: `src/` → `crates/aom/src/`, then `main.rs`/`cli.rs` → `crates/cli/`, `tests/` → `crates/aom/tests/`
- Edit: `crates/aom/src/lib.rs`

**Interfaces:**

- Consumes: existing public API `agent_o_matic::generate::{Action, FileReport, Report, Options, run}` (all `pub`; `Action::label(self) -> &'static str` is `pub`, see `generate.rs:22`).
- Produces: workspace member `agent-o-matic` (lib crate `agent_o_matic`) consumed by `crates/cli` and (later) `crates/orchestrator`; binary `aom` in package `aom-cli`.

- [ ] **Step 1: Create the crate directories and move the source tree with git**

```bash
cd "$(git rev-parse --show-toplevel)"
mkdir -p crates/cli/src crates/orchestrator/src
git mv src crates/aom/src
git mv tests crates/aom/tests
git mv crates/aom/src/main.rs crates/cli/src/main.rs
git mv crates/aom/src/cli.rs crates/cli/src/cli.rs
```

- [ ] **Step 2: Rewrite the root `Cargo.toml` as a virtual workspace**

Replace the entire contents of `Cargo.toml` with:

```toml
[workspace]
resolver = "3"
members = ["crates/aom", "crates/cli"]

[workspace.package]
edition = "2024"
rust-version = "1.95"
license = "MIT"
repository = "https://github.com/constantin-jais/Agent-O-Matic"

[workspace.dependencies]
blake3 = "1"
clap = { version = "4", features = ["derive"] }
miette = { version = "7", features = ["fancy"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
toml = "0.8"
tempfile = "3"
agent_o_matic = { path = "crates/aom" }

[profile.release]
strip = true
lto = "thin"
```

- [ ] **Step 3: Create `crates/aom/Cargo.toml`** (compiler library, no `clap`)

```toml
[package]
name = "agent-o-matic"
version = "0.0.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true
repository.workspace = true
description = "A clean-room, deterministic, agent-agnostic configuration compiler: one declarative source, many AI-agent configs, with safe-write and drift detection."
keywords = ["ai", "agents", "codegen", "claude", "agents-md"]
categories = ["command-line-utilities", "development-tools"]

[lib]
name = "agent_o_matic"
path = "src/lib.rs"

[dependencies]
blake3.workspace = true
miette.workspace = true
serde.workspace = true
serde_json.workspace = true
thiserror.workspace = true
toml.workspace = true

[dev-dependencies]
tempfile.workspace = true
```

> Note: `readme` is intentionally dropped here (the front-page `README.md` stays at repo root). Re-add a crate-local `readme` when we first publish to crates.io.

- [ ] **Step 4: Edit `crates/aom/src/lib.rs`** — remove the CLI concern from the library

Replace the file's module/use/`run` region so it reads exactly:

```rust
//! Agent-O-Matic — a deterministic, agent-agnostic configuration compiler.
//!
//! One declarative source (a TOML manifest + referenced Markdown files) is
//! compiled into configuration for many AI coding agents (AGENTS.md today;
//! Claude Code, Cursor, … later). The distinctive subsystems are *safe-write*
//! (never clobber a hand-edited generated file) and *drift detection*
//! (regeneration is reproducible and verifiable in CI).
//!
//! This crate is built clean-room as a learning/teaching artifact: every
//! non-obvious decision is recorded in `docs/adr/`, and the tests are the
//! executable specification.
//!
//! ## Pipeline
//!
//! `parse` → `resolve` includes → build `ir` → `merge` by priority →
//! `render` per adapter → `safe_write` (guarded by the `lock`) → `audit`.

mod audit;
pub mod config;
pub mod error;
pub mod generate;
mod ir;
mod lock;
mod merge;
mod paths;
pub mod render;
mod resolve;
mod safe_write;

pub use error::{Error, Result};
```

(Removes `mod cli;`, the `use clap::Parser;` / `use cli::{Cli, Command};` lines, and the entire `pub fn run()`.)

- [ ] **Step 5: Create `crates/cli/Cargo.toml`** (the `aom` binary)

```toml
[package]
name = "aom-cli"
version = "0.0.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[[bin]]
name = "aom"
path = "src/main.rs"

[dependencies]
agent_o_matic.workspace = true
clap.workspace = true
miette.workspace = true
```

- [ ] **Step 6: Rewrite `crates/cli/src/main.rs`** — parse args, dispatch (logic lifted verbatim from the old `lib.rs::run`)

```rust
//! The `aom` binary: parse args, dispatch to the compiler, print a report.

mod cli;

use clap::Parser;
use cli::{Cli, Command};

use agent_o_matic::generate;

fn main() -> miette::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Generate {
            manifest,
            check,
            force,
        } => {
            let report = generate::run(&generate::Options {
                manifest_path: manifest,
                check,
                force,
            })?;
            for file in &report.files {
                println!("{:>9}  {}", file.action.label(), file.path);
            }
            if check {
                println!("ok: {} target(s) up to date", report.files.len());
            }
            Ok(())
        }
    }
}
```

`crates/cli/src/cli.rs` is the unchanged file moved in Step 1 (`Cli` + `Command::Generate`).

- [ ] **Step 7: Build the workspace**

Run: `cargo build --workspace`
Expected: compiles clean; binary at `target/debug/aom`.

- [ ] **Step 8: Run the full test suite (the regression gate)**

Run: `cargo test --workspace`
Expected: `42 passed` (lib unit) + `3 passed` (e2e) + `0 failed`, identical to baseline.

- [ ] **Step 9: Verify lint + format + compiler non-regression**

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p aom-cli -- generate --manifest examples/minimal/harness.toml --check
```

Expected: fmt silent (exit 0); clippy clean; drift check prints `ok: 1 target(s) up to date`.

- [ ] **Step 10: Commit**

```bash
git add -A
git commit -m "refactor: split agent-o-matic into a cargo workspace (aom lib + cli bin)"
```

---

### Task 2: Scaffold the `crates/orchestrator` library

The home for A1+ (goals/gates, then incident/issue/dispatch). Additive — keeps the build green on its own.

**Files:**

- Create: `crates/orchestrator/Cargo.toml`, `crates/orchestrator/src/lib.rs`
- Modify: `Cargo.toml` (add member)

**Interfaces:**

- Consumes: nothing yet (depends on `agent_o_matic` by path for the future drift gate, but uses none of it in A0).
- Produces: crate `orchestrator` with `pub const CRATE_NAME: &str` (placeholder anchor that A1 replaces with real modules).

- [ ] **Step 1: Write the failing smoke test** in `crates/orchestrator/src/lib.rs`

```rust
//! Orchestrator — the agentic CI/CD control loop built on top of the
//! `agent_o_matic` compiler. Phases A1+ add: goals & gates, incident model,
//! GitHub issue bridge, and Claude-Code dispatch. A0 ships only this scaffold.

/// Stable crate identity used by early wiring tests; replaced by real modules in A1.
pub const CRATE_NAME: &str = "orchestrator";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_member_links() {
        assert_eq!(CRATE_NAME, "orchestrator");
    }
}
```

- [ ] **Step 2: Create `crates/orchestrator/Cargo.toml`**

```toml
[package]
name = "orchestrator"
version = "0.0.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[dependencies]
agent_o_matic.workspace = true
```

- [ ] **Step 3: Add the member to the workspace** — edit root `Cargo.toml`:

```toml
members = ["crates/aom", "crates/cli", "crates/orchestrator"]
```

- [ ] **Step 4: Run the orchestrator test**

Run: `cargo test -p orchestrator`
Expected: `1 passed` (`workspace_member_links`).

- [ ] **Step 5: Re-run the whole workspace green**

Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings`
Expected: all green; clippy clean (the unused `agent_o_matic` dep is allowed — it is a path dep, not an unused `use`).

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat: scaffold crates/orchestrator (A1+ agentic loop home)"
```

---

### Task 3: Make CI workspace-aware

**Files:**

- Modify: `.github/workflows/ci.yml`

**Interfaces:**

- Consumes: nothing.
- Produces: a CI job that lints/tests every workspace member.

- [ ] **Step 1: Update the three cargo invocations** in `.github/workflows/ci.yml` — replace the `Format`/`Lint`/`Test` step `run:` lines:

```yaml
- name: Format
  run: cargo fmt --all --check
- name: Lint
  run: cargo clippy --workspace --all-targets --all-features
- name: Test
  run: cargo test --workspace --all-features
```

(`--all` → `--workspace` for clippy/test; `fmt --all` already covers every member.)

- [ ] **Step 2: Sanity-run the same commands locally**

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features
cargo test --workspace --all-features
```

Expected: all green (mirrors what CI will run; `RUSTFLAGS=-D warnings` is set in the workflow env).

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: make fmt/clippy/test workspace-aware"
```

---

### Task 4: Record ADR-0006 (workspace + orchestrator charter)

Documents _why_ the workspace exists and why ADR-0001 still binds `crates/aom`. Required by CONTRIBUTING ("any architectural change ships with a new ADR").

**Files:**

- Create: `docs/adr/0006-workspace-and-orchestrator-charter.md`

- [ ] **Step 1: Write the ADR**

```markdown
# ADR-0006: Workspace split and the orchestrator charter

## Status

Accepted (2026-06-27).

## Context

Agent-O-Matic began as a single crate: a clean-room, deterministic
configuration compiler whose charter (ADR-0001) explicitly excludes an MCP
server, a remote registry, and agent orchestration — these were named as
gold-plating against the teaching goal.

A new, separate ambition has appeared: an **open-source agentic CI/CD loop**
(incident → issue → hand-off to a fixer agent → hard gates → merge → deploy →
verify → rollback), built _on top of_ the compiler and dogfooded by it. This
concern is orthogonal to "compile one source into many agent configs".

## Decision

Restructure the repository into a Cargo workspace rather than growing the
compiler crate:

- `crates/aom` — the compiler library `agent_o_matic`, **unchanged in spirit
  and still governed by ADR-0001**. It gains no orchestration, no MCP, no
  network dependency. It loses only its CLI wiring (moved out), which sharpens
  its identity as a pure library.
- `crates/cli` — the `aom` binary; a thin application layer that wires the
  compiler (and, later, the orchestrator) behind a clap CLI.
- `crates/orchestrator` — the new concern: goals & gates (A1), then the
  incident/issue/dispatch loop (A3+). Its charter is separate from ADR-0001.

ADR-0001 is therefore **not superseded**: it continues to describe the
compiler crate exactly. This ADR adds the workspace and a distinct charter for
the orchestrator.

## The orchestrator's safety envelope (binding from A5 onward)

Autonomy is permitted only inside a hard, reversible envelope:

1. Hard gates are blocking and evidence-backed (nothing merges/deploys without
   attached green proof).
2. Deploys are reversible with automatic rollback (canary → smoke → rollback).
3. A circuit-breaker bounds blast radius (deploy rate-limit, max fix attempts,
   global kill-switch).
4. Every autonomous action is recorded in a zero-PII audit trail.
5. A scope-fence restricts the loop to an allowlist of repos/targets; it never
   touches infrastructure credentials.

## Consequences

- One-time restructure cost; the compiler's tests and guarantees are unchanged
  (the existing suite is the regression gate).
- The compiler stays publishable and teachable on its own; the orchestrator can
  depend on it without polluting it.
- Profiles (`[profile.release]`) now live only in the workspace root, per Cargo.
```

- [ ] **Step 2: Commit**

```bash
git add docs/adr/0006-workspace-and-orchestrator-charter.md
git commit -m "docs(adr): 0006 workspace split and orchestrator charter"
```

---

## Self-Review

**Spec coverage:** A0 spec items — workspace refactor (Task 1), orchestrator scaffold (Task 2), CI workspace-aware (Task 3), ADR (Task 4), green baseline preserved (Steps 8–9 of Task 1, Step 5 of Task 2, Step 2 of Task 3). Covered.

**Placeholder scan:** No "TBD"/"handle edge cases"/uncoded steps — every code/command step shows exact content. `CRATE_NAME` is the one intentional placeholder _value_, explicitly flagged as replaced in A1.

**Type consistency:** `crates/cli/src/main.rs` calls `generate::run`, `generate::Options { manifest_path, check, force }`, `report.files`, `file.action.label()`, `file.path` — all match `generate.rs` (`Options` fields line 56-63; `Report.files` line 51; `FileReport.path/action` line 43-46; `Action::label` line 22). `Cli`/`Command::Generate { manifest, check, force }` match the moved `cli.rs`.

**Risks flagged:** Task 1 is atomic-large by necessity (workspace move); the 45-test suite is its gate. `cargo run` now needs `-p aom-cli` (or a future `default-run`); reflected in Step 9.

---

## Execution Handoff

Run this on an isolated **git worktree** (Task 1 breaks `main`'s build mid-refactor, and the user is actively committing to this repo): create it via `superpowers:using-git-worktrees` at execution time, off `origin/main`.

Next plans (separate files, after A0 lands): **A1** goals & gates (`crates/orchestrator`: schema, gate eval, `aom goals report`, `aom gate run`); then **A3+A4** incident→issue (octocrab) → dispatch (`claude -p` in worktree) → PR.
