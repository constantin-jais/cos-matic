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
