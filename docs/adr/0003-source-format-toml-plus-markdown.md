# ADR-0003 — Source format: TOML manifest + referenced Markdown files

- Status: accepted
- Date: 2026-06-27

## Context

The source-of-truth is the interface users (and learners) read and write. It
must declare structure (domains, profiles, targets) _and_ carry instruction
content (which is itself Markdown, often long). Options considered:

1. TOML manifest, content inline via triple-quoted strings.
2. **TOML manifest + content in separate `.md` files, referenced by path.**
3. Markdown files with YAML frontmatter, no central manifest (directory convention).
4. YAML manifest + `.md` files.

## Decision

**Option 2: a TOML manifest declares the structure; domain _content_ lives in
separate Markdown files referenced by path.**

A domain may provide its content either inline (`content = "..."`, fine for a
couple of lines) or by reference (`content_file = "domains/code-style.md"`,
preferred for real prose).

## Rationale

- Long Markdown inside TOML triple-quoted strings is unreadable and produces
  noisy diffs. Separating prose into `.md` keeps both the manifest and the
  content clean and individually diffable — the same split the prior
  `governance/prompt-system` used successfully (`conformite.md`, `stack.md`, …).
- TOML gives explicit, typed structure without YAML's indentation fragility —
  consistent with the determinism/rigor wedge.
- A central manifest (vs. option 3's pure convention) makes composition
  explicit and auditable, which matters for the safe-write/drift story.

## Consequences

- The schema needs a `Domain` with optional `content` xor `content_file`
  (exactly one), validated with a clear diagnostic.
- Content files are resolved relative to the manifest's directory; paths must
  stay inside the project (no machine-local absolute paths — see ADR: safe-write-sentinel-lockfile / the
  no-local-paths requirement).
- YAML manifest (option 4) rejected: weaker typing, indentation fragility.
