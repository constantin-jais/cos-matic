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

    /// Incident handling (open an idempotent GitHub issue).
    Incident {
        #[command(subcommand)]
        command: IncidentCommand,
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

#[derive(Debug, Subcommand)]
pub enum IncidentCommand {
    /// Create or reuse a GitHub issue for an incident (idempotent by fingerprint).
    Open {
        /// Incident class, e.g. `gate-red`, `ci-failure`.
        #[arg(long)]
        kind: String,

        /// Issue title.
        #[arg(long)]
        title: String,

        /// Issue body (markdown).
        #[arg(long, default_value = "")]
        body: String,

        /// Severity recorded in the journal, e.g. low | medium | high.
        #[arg(long, default_value = "medium")]
        severity: String,

        /// Fingerprint key (defaults to the title); same kind+key reuses the same issue.
        #[arg(long)]
        key: Option<String>,

        /// Target repo `owner/name` (defaults to the `origin` remote).
        #[arg(long)]
        repo: Option<String>,

        /// Label to attach (repeatable). Must already exist on the repo.
        #[arg(long = "label")]
        labels: Vec<String>,
    },
}
