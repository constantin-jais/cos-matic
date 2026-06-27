//! Command-line surface for the `aom` binary.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "aom",
    version,
    about = "Agent-O-Matic: compile one source into many AI-agent configs (safe-write, drift-aware)."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Compile the manifest into every declared target.
    Generate {
        /// Path to the root manifest.
        #[arg(short, long, default_value = "harness.toml")]
        manifest: PathBuf,

        /// Verify outputs are up to date without writing anything (CI gate).
        #[arg(long)]
        check: bool,

        /// Overwrite files that were hand-edited since the tool last wrote them.
        #[arg(long)]
        force: bool,
    },

    /// Goals & metrics reporting.
    Goals {
        #[command(subcommand)]
        command: GoalsCommand,
    },

    /// Run the blocking gate-wall.
    Gate {
        #[command(subcommand)]
        command: GateCommand,
    },
}

#[derive(Debug, Subcommand)]
pub enum GoalsCommand {
    /// Render a Markdown report of the phase, milestones, gates and observability.
    Report {
        /// Path to the goals file.
        #[arg(short, long, default_value = "goals.toml")]
        config: PathBuf,
    },
}

#[derive(Debug, Subcommand)]
pub enum GateCommand {
    /// Run fmt + clippy + tests and exit non-zero on any red hard gate.
    Run {
        /// Path to the goals file declaring the hard gates.
        #[arg(short, long, default_value = "goals.toml")]
        config: PathBuf,
    },
}
