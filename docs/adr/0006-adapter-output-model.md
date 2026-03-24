# ADR-0006 — Adapter output model: a set of files, not one string

- Status: accepted
- Date: 2026-06-27

## Context

Phase 1's `Adapter::render` returned a single `String` (one AGENTS.md). Looking
ahead to more targets revealed three genuinely different output _shapes_:

- **Pi** (pi.dev) reads `AGENTS.md`/`CLAUDE.md` natively, plus an optional
  `.pi/settings.json` and `SYSTEM.md` → one-or-two files.
- **Cursor** writes a _directory_ of `.cursor/rules/*.mdc`, one per rule → N files.
- **OpenForge** (a plugin _distribution_ marketplace) consumes a `.claude-plugin/`
  _package_: a `plugin.json` manifest plus a directory structure → a package.
- **Clever AI** is an OpenAI-compatible inference _endpoint_, configured by API
  params (base_url/model/key) → not an instruction file at all (runtime concern).

A `String`-returning adapter cannot express N-file or package outputs.

## Decision

An adapter produces **`Vec<RenderedFile>`** (each `{ path, content }`, repo-relative),
plus collected **warnings**. Zero, one, or many files; arbitrary paths.

```rust
struct RenderedFile { path: String, content: String }
struct RenderOutput { files: Vec<RenderedFile>, warnings: Vec<String> }
trait Adapter {
    fn id(&self) -> &'static str;
    fn supports(&self, feature: Feature) -> bool;
    fn render(&self, input: &RenderInput) -> Result<RenderOutput>;
}
```

Each `RenderedFile` goes through the same safe-write + lock + audit path as before,
so the safety guarantees extend to every target uniformly.

## Consequences

- Adding a target is a localized change: implement `Adapter`, register it. This is
  the extensibility the project is aiming for.
- Single-file adapters (universal, claude, pi) return a one-element vector;
  multi-file adapters (cursor) return many; package adapters (openforge, later)
  return a manifest plus files — all without touching the engine.
- **Target classification** (what we build now vs. document as extension points):
  - _File-instruction targets_ (build): `universal`/AGENTS.md, `claude`/CLAUDE.md,
    `cursor`, and Pi (reuses AGENTS.md + a JSON settings file).
  - _Distribution target_ — **OpenForge**: a different output shape (`.claude-plugin/`
    package). The `Vec<RenderedFile>` model can express it; deferred until the
    plugin format is pinned down. Not built speculatively (anti-gold-plating).
  - _Runtime/wiring concern_ — **Clever AI**: an endpoint, not instructions. A thin
    adapter could emit an env snippet (base_url/model), but that is a deployment
    concern, not config generation. Out of scope for now; documented so the
    boundary is explicit.
- Warnings are first-class (not stderr noise): they are collected and surfaced in
  the run report, keeping the tool deterministic and auditable.
