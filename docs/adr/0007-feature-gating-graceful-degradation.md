# ADR-0007 — Feature gating via capabilities + graceful degradation

- Status: accepted
- Date: 2026-06-27

## Context

Some targets have capabilities others lack. The first concrete case is Cursor's
**glob-based activation**: a `.cursor/rules/*.mdc` file can declare `globs` so the
rule only applies to matching files. AGENTS.md / CLAUDE.md have no such concept —
their content always applies. Later, Claude-only features (subagents, hooks,
output-styles) will pose the same question.

So a domain may carry optional metadata (e.g. `globs`) that only some adapters can
honor. What happens when a selected target cannot express it?

## Decision

A small capability model with **graceful degradation (warn + omit)**:

- A `Feature` enum enumerates gateable capabilities (`GlobActivation` today;
  `Subagents`, `Hooks` later).
- `Adapter::supports(feature) -> bool` declares what each adapter can honor.
- When a domain declares metadata for a feature the target adapter does **not**
  support, the adapter renders the content **without** that feature and records a
  **warning** (it never errors, and never silently drops the metadata unseen).

Rejected alternatives:

- _Strict (error on unsupported feature)_: safer in the abstract, but it forces the
  user to fork their source per target, which defeats "one source, many agents".
  A future `--strict` flag could opt into this; not the default.
- _Per-target fields in the manifest_ (no capability abstraction): simplest, but it
  couples the manifest to every target and cannot express "this metadata is honored
  by some, ignored-with-a-warning by others" cleanly.

## Consequences

- Warnings are collected into the run report (see ADR-0006), so degradation is
  visible and auditable — consistent with the determinism/auditability wedge.
- Adding a feature = extend the `Feature` enum, implement `supports` in each
  adapter, and have the adapter that owns it consume the metadata. Other adapters
  automatically degrade-with-warning.
- `Domain` gains optional `globs`; adapters that don't support `GlobActivation`
  warn and render the content unconditionally.
