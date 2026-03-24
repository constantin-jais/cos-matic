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
mod cli;
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

use clap::Parser;
use cli::{Cli, Command};

/// Entry point used by the `aom` binary: parse args, dispatch, print a report.
pub fn run() -> miette::Result<()> {
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
            for warning in &report.warnings {
                eprintln!("warning: {warning}");
            }
            if check {
                println!("ok: {} file(s) up to date", report.files.len());
            }
            Ok(())
        }
    }
}
