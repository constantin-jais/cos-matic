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

    /// Incident handling (open an idempotent GitHub issue).
    Incident {
        #[command(subcommand)]
        command: IncidentCommand,
    },

    /// Dispatch a bounded fix attempt for an issue (isolated branch; never merges).
    Dispatch {
        /// Issue number to address.
        #[arg(long)]
        issue: u64,

        /// Short title of the fix.
        #[arg(long)]
        title: String,

        /// Context handed to the fixer (markdown).
        #[arg(long, default_value = "")]
        body: String,

        /// Target repo `owner/name` (defaults to the `origin` remote).
        #[arg(long)]
        repo: Option<String>,
    },

    /// Autonomously merge a branch — only with attached green evidence (never red).
    Automerge {
        /// Branch to gate-and-merge (e.g. `aom/fix/issue-8`).
        #[arg(long)]
        branch: String,

        /// Target repo `owner/name` (defaults to the `origin` remote).
        #[arg(long)]
        repo: Option<String>,
    },

    /// Canary-deploy a target, smoke-test it, and promote or auto-rollback.
    Deploy {
        /// Version/ref to deploy (exported to the deploy command as `AOM_TARGET`).
        #[arg(long)]
        target: String,

        /// Target repo `owner/name` (defaults to the `origin` remote).
        #[arg(long)]
        repo: Option<String>,
    },

    /// Run the end-to-end loop: dispatch -> publish -> automerge -> deploy,
    /// retried until it lands or the iteration budget is spent.
    Loop {
        /// Issue number driving the loop.
        #[arg(long)]
        issue: u64,

        /// Short title of the fix.
        #[arg(long)]
        title: String,

        /// Context handed to the fixer (markdown).
        #[arg(long, default_value = "")]
        body: String,

        /// Target repo `owner/name` (defaults to the `origin` remote).
        #[arg(long)]
        repo: Option<String>,

        /// Trace what each stage would do — real read-only checks (the merge
        /// gate), but no fix, no merge, no deploy.
        #[arg(long)]
        dry_run: bool,

        /// Retry the loop up to this many times before giving up (circuit-breaker).
        #[arg(long, default_value_t = 3)]
        max_iterations: u32,
    },

    /// Initialize a new Agent-O-Matic project (interactive setup wizard).
    Init {
        /// Project name (skips prompt if provided).
        #[arg(long)]
        name: Option<String>,

        /// Autonomy level: L0, L1, L2, or L3.
        #[arg(long, value_name = "LEVEL")]
        level: Option<String>,

        /// Adapter to include (repeatable): universal, claude, or cursor.
        #[arg(long = "adapter", value_name = "ADAPTER")]
        adapters: Vec<String>,

        /// GitHub repo in format owner/name (optional).
        #[arg(long, value_name = "REPO")]
        repo: Option<String>,

        /// Skip all prompts; use flags + defaults (non-interactive mode).
        #[arg(long)]
        yes: bool,
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
