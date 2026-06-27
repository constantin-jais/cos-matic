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
pub mod goals;
mod ir;
pub mod library;
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
            print_goals(&report.goals);
            if check {
                println!("ok: {} file(s) up to date", report.files.len());
            }
            Ok(())
        }
        Command::Library { action } => match action {
            cli::LibraryAction::List => {
                for (name, priority, description) in library::catalog() {
                    println!("{name:<20} (priority {priority:>3})  {description}");
                }
                Ok(())
            }
            cli::LibraryAction::Show { name } => {
                print!("{}", library::content(&name)?);
                Ok(())
            }
        },
        Command::Goals { manifest } => {
            let (_root, manifest, tree) = generate::load_tree(&manifest)?;
            let outcomes = goals::evaluate(&tree, &manifest.goals)?;
            print_goals(&outcomes);
            let failures: Vec<String> = outcomes
                .iter()
                .filter(|o| o.is_blocking_failure())
                .map(|o| format!("  {}: {}", o.check, o.detail))
                .collect();
            if failures.is_empty() {
                Ok(())
            } else {
                Err(Error::GoalsFailed { failures }.into())
            }
        }
    }
}

/// Print one line per goal outcome, marking hard-gate failures.
fn print_goals(outcomes: &[goals::GoalOutcome]) {
    use crate::config::schema::GoalKind;
    for o in outcomes {
        let kind = match o.kind {
            GoalKind::HardGate => "hard_gate",
            GoalKind::Observability => "observability",
        };
        let status = if o.passed { "PASS" } else { "FAIL" };
        println!("goal [{kind}] {status}  {}: {}", o.check, o.detail);
    }
}
