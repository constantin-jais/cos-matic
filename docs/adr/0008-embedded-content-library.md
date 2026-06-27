# ADR-0008 — Embedded content library: `builtins` + `library://`

- Status: accepted
- Date: 2026-06-27

## Context

The tool's distinctive value is not the engine (that paradigm is a commodity —
ADR-0001) but a curated, reusable set of neutral instruction **domains**. Users
should be able to compose those without authoring the prose themselves. Two
questions: how is the content shipped, and how does a manifest reference it?

## Decision

**Shipped: embedded in the binary** via `include_str!` over `content/domains/*.md`.
One static binary, no network, air-gap friendly (consistent with ADR-0006). The
content lives in-repo (reviewable, versioned with the tool) and is registered in
a small Rust table (`name`, `priority`, `description`, content).

**Referenced: two surfaces over one engine.**

- `[[includes]] path = "library://four-axes"` — the canonical form; the resolver
  recognizes the `library://` scheme and adds the built-in domain instead of
  reading a file. One composition mechanism (the same `[[includes]]` as Phase 1).
- `[package] builtins = ["four-axes", "tdd"]` — concise sugar that desugars to
  `library://` includes. Discoverable, terse for the common case.

Built-in domains are de-duplicated by name during resolution (pulling the same
built-in from two manifests is a no-op, not a `DuplicateName` error). A
user-defined domain colliding with a built-in name is still a real clash.

## Consequences

- Adding a built-in = drop a `content/domains/<name>.md` and one registry row;
  it is then available via both surfaces and listed by `aom library list`.
- `aom library list` / `aom library show <name>` let users discover and read the
  content without scaffolding anything.
- Updating built-ins ships with a new binary release (acceptable; no remote
  registry — that would be premature, see ADR-0001's anti-adoption-race stance).
- Seed set (neutral, English, migrated from the author's `~/.claude/`): the four
  decision axes, response blocks, factual style, agent behavior, code style,
  security baseline, TDD, anti-gold-plating.
- `aom init` (scaffold a starter `harness.toml`) is a thin follow-up once the
  library exists; not required for the library itself.
