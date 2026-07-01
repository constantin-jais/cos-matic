# Cosmatic Routine Harness Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `bolt-cosmatic` usable as a routine local command and make this repository dogfood a root `harness.toml`.

**Architecture:** Keep the change narrow: fix stale binary references, add regression tests around CLI ergonomics, add a root manifest that compiles deterministic agent configuration, and document the session bootstrap. The root manifest uses the existing embedded library and safe-write generator rather than inventing a second configuration path.

**Tech Stack:** Rust 2024 workspace, Cargo integration tests, `bolt-cosmatic generate`, shell scripts, Markdown/TOML configuration.

## Global Constraints

- Security first: no secrets in committed files, live autonomous loop remains sandbox-only.
- Quality: TDD for behavior changes, `cargo fmt`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --all-features`.
- Performance: no new runtime dependency or network call in the routine local gates.
- Completeness: code + tests + docs + generated agent config from the root manifest.

---

### Task 1: Pin Routine CLI Ergonomics

**Files:**
- Modify: `crates/cli/tests/cli_behavior.rs`
- Modify: `scripts/live-loop-smoke.sh`
- Modify: `crates/cli/src/cli.rs`

**Interfaces:**
- Consumes: existing `CARGO_BIN_EXE_bolt-cosmatic` integration-test binary.
- Produces: regression coverage that refuses stale `--bin bolt-cosmatic` script calls and stale help naming.

- [ ] **Step 1: Write failing tests**

Add tests that read `scripts/live-loop-smoke.sh` and assert it uses `--bin bolt-cosmatic`, and that `bolt-cosmatic --help` contains `Usage: bolt-cosmatic`.

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test -p bolt-cos-matic-cli cli_help_uses_bolt-cosmatic_name live_loop_smoke_uses_bolt-cosmatic_binary -- --nocapture`

Expected: failure because the script still contains `--bin bolt-cosmatic`.

- [ ] **Step 3: Write minimal implementation**

Replace `--bin bolt-cosmatic` with `--bin bolt-cosmatic` in `scripts/live-loop-smoke.sh`; set the clap command name to `bolt-cosmatic`.

- [ ] **Step 4: Run tests to verify pass**

Run: `cargo test -p bolt-cos-matic-cli cli_help_uses_bolt-cosmatic_name live_loop_smoke_uses_bolt-cosmatic_binary -- --nocapture`

Expected: pass.

### Task 2: Dogfood Root Harness

**Files:**
- Create: `harness.toml`
- Create: `domains/core-values.md`
- Generated: `AGENTS.md`
- Generated: `CLAUDE.md`
- Generated: `.cursor/rules/default.mdc`
- Modify: `crates/cli/tests/cli_behavior.rs`

**Interfaces:**
- Consumes: `bolt-cosmatic generate --manifest harness.toml`.
- Produces: a root manifest and generated agent config that can be checked by CI and used by Codex sessions.

- [ ] **Step 1: Write failing test**

Add a test that runs `bolt-cosmatic generate --check --manifest <repo>/harness.toml` from the repository root.

- [ ] **Step 2: Run test to verify failure**

Run: `cargo test -p bolt-cos-matic-cli root_harness_is_present_and_in_sync -- --nocapture`

Expected: failure because no root `harness.toml` exists yet.

- [ ] **Step 3: Write minimal implementation**

Add `harness.toml`, `domains/core-values.md`, then run `bolt-cosmatic generate --manifest harness.toml` to create committed outputs.

- [ ] **Step 4: Run test to verify pass**

Run: `cargo test -p bolt-cos-matic-cli root_harness_is_present_and_in_sync -- --nocapture`

Expected: pass.

### Task 3: Document Routine Codex Usage

**Files:**
- Create: `docs/codex-routine.md`
- Modify: `README.md`
- Modify: `CONTRIBUTING.md`

**Interfaces:**
- Consumes: installed `bolt-cosmatic` command or fallback `cargo run --bin bolt-cosmatic --`.
- Produces: a reproducible session bootstrap and local gate sequence.

- [ ] **Step 1: Add documentation**

Document the daily commands: install, session start, pre-change gates, post-change gates, and live-loop restrictions.

- [ ] **Step 2: Run full verification**

Run:

```bash
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
cargo run -q --bin bolt-cosmatic -- generate --check --manifest harness.toml
cargo run -q --bin bolt-cosmatic -- goals --manifest harness.toml
```

Expected: all pass.

- [ ] **Step 3: Install command**

Run: `cargo install --path crates/cli`

Expected: `bolt-cosmatic --help` works from `/Users/ifi6567/Documents`.
