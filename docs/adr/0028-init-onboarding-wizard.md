# ADR-0028 — Interactive onboarding wizard (`aom init`)

## Status

Accepted (2026-06-29). Enables L0→L3 entry ramp with zero manual scaffolding.

## Context

New users face a blank slate: creating `harness.toml`, domain files, GitHub workflows (for L1+), and understanding autonomy levels requires reading ADRs and examples. Onboarding friction grows with autonomy level—L3 involves policy configuration, sandbox setup, and workflow permissions.

The ADR hierarchy (ADR: north-star-trustworthy-autonomy) defines four autonomy levels as a trust-building ramp: L0 (compile-only) → L1 (bounded dispatch) → L2 (gated loop) → L3 (trusted auto). Yet there was no first-run tool to guide users through the choice or scaffold the scaffolding.

## Decision

Introduce `aom init`: an interactive Rust-native wizard that:

1. **Onboards users interactively** (or non-interactively via `--yes`).
   - Guides choice of project name, autonomy level, adapters, and GitHub repo.
   - Runs in TUI mode (inquire) by default; skips prompts when flags set.
   - Non-interactive mode (`--yes`) requires `--name`; other inputs default (level: L0, adapter: universal).

2. **Scaffolds safe-write artifacts** (never clobbers).
   - `harness.toml` with `[package]` name, sample `[[domains]]`, `[[profiles]]`, `[[targets]]` (per adapter), `[autonomy] level`.
   - `domains/core-values.md` sample domain tailored to the project.
   - `.github/workflows/orchestrator-loop.yml` for L1/L2/L3 (copied from repo template; L0 omits it).
   - Simple string replacement for templates (`{{name}}`, `{{level}}`, `{{adapter_targets}}`), no template engine.

3. **Prints a manual operator checklist** (L1/L2/L3).
   - Does not automate GitHub settings, repo vars, or CI permissions—user applies them manually.
   - Checklist flags each setup step: commit scaffold, review manifest, run `aom generate --check`, set repo vars (L2/L3), configure branch protections, etc.

4. **Respects safe-write invariant** (ADR: safe-write-sentinel-lockfile).
   - If `harness.toml`, workflows, or domains exist, warns and skips; never overwrites.
   - Second run on an initialized project is a no-op with a friendly message.

## Consequences

- **Entry ramp is smooth:** L0 → compile, L1 → dispatch (with guided workflow setup), L2/L3 → full loop (with policy scaffold and manual gate list). Every level is additive, not a rewrite.
- **Wizard is local & portable:** no bootstrap server, no remote state—purely CLI. Users fork with their own scaffold defaults in `crates/cli/templates/`.
- **Interactivity is optional:** `--yes` mode works in CI/scripting; TUI for humans. Both paths validate inputs identically.
- **Autonomy level becomes concrete:** `[autonomy] level` is now a first-class field in every `harness.toml`, even if the orchestrator is deferred. Policy & audit (ADR: architecture-targets-seams-isolation-durability, §3) can read it at runtime.
- **Safe-write is reinforced:** init is the first place users create artifacts; it must never corrupt existing projects. Same pattern as `aom generate --force`—safe-write is the default, force is opt-in (and forbidden here, as init always respects existing files).
- **Templates live alongside code:** Under `crates/cli/templates/`, version-controlled and forkable. Changes to the template are shipped with the binary via `include_str!`.

## Cross-references

- (ADR: north-star-trustworthy-autonomy) — L0–L3 ramp definition and persona (solo → team).
- (ADR: safe-write-sentinel-lockfile) — init respects safe-write by never clobbering existing artifacts.
- (ADR: architecture-targets-seams-isolation-durability) — autonomy level as typed config.
- (ADR: end-to-end-loop) — workflow template is generic, trigger-agnostic.
