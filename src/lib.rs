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

use miette::Result;

/// Entry point used by the `aom` binary. Phase 0 placeholder.
pub fn run() -> Result<()> {
    Ok(())
}
