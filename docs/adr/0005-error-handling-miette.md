# ADR-0005 — Error handling: `miette` diagnostics from the start

- Status: accepted
- Date: 2026-06-27

## Context

This is a _compiler_: errors should point at the exact location in the source
(`harness.toml` or a referenced `.md`) and explain what to fix. Options:

1. `anyhow` everywhere (flat, fast to write).
2. `thiserror` typed errors in the library + `anyhow` at the binary boundary.
3. **`miette` from the start** (rich diagnostics: source spans, labels, help),
   with `thiserror` for the typed error enums it builds on.

## Decision

**Option 3: `miette` + `thiserror` from line one.**

Library error types derive `thiserror::Error` and `miette::Diagnostic`; they
carry a `#[source_code]` and `#[label]` so a TOML parse error or a dangling
reference renders with the offending span underlined and a `help` hint.

## Rationale

- Designing good diagnostics is itself a core learning objective; doing it from
  the start avoids a later rewrite of every module's error type.
- Spans turn "invalid config" into "domain `security` referenced by profile
  `default` is not defined (line 12)" — the difference between a toy and a tool.
- Determinism/clarity wedge: precise, reproducible error messages.

## Consequences

- Slightly more ceremony per error variant up front.
- The `toml` crate exposes byte spans on parse errors; we map them into
  `miette::SourceSpan` so parse failures get a pointed label.
- Errors must never embed machine-local absolute paths (repo-relative only).
