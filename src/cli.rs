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

    /// Inspect the embedded content library.
    Library {
        #[command(subcommand)]
        action: LibraryAction,
    },

    /// Evaluate the declared goals without writing anything (a CI gate).
    Goals {
        /// Path to the root manifest.
        #[arg(short, long, default_value = "harness.toml")]
        manifest: PathBuf,
    },
}

#[derive(Debug, Subcommand)]
pub enum LibraryAction {
    /// List every built-in domain.
    List,

    /// Print a built-in domain's content.
    Show {
        /// Built-in name (see `aom library list`).
        name: String,
    },
}
