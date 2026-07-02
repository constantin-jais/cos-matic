//! Command-line surface for the `bolt-cosmatic` binary.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "bolt-cosmatic",
    version,
    about = "bolt-cos-matic: compile one source into many AI-agent configs (safe-write, drift-aware)."
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

    /// Validate or report Rumble delivery maturity claims (read-only; never promotes).
    Maturity {
        #[command(subcommand)]
        action: MaturityAction,
    },

    /// Validate or dry-run plan a Rumble-to-Bolt handoff payload (never executes).
    Handoff {
        #[command(subcommand)]
        action: HandoffAction,
    },

    /// Run deterministic repository inspections.
    Inspect {
        #[command(subcommand)]
        action: InspectAction,
    },

    /// Run local-only stack validation helpers (no provisioning, no provider calls).
    Stack {
        #[command(subcommand)]
        action: StackAction,
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
        /// Branch to gate-and-merge (e.g. `bolt/run/run-1/issue-8/attempt-1`).
        #[arg(long)]
        branch: String,

        /// Target repo `owner/name` (defaults to the `origin` remote).
        #[arg(long)]
        repo: Option<String>,
    },

    /// Canary-deploy a target, smoke-test it, and promote or auto-rollback.
    Deploy {
        /// Version/ref to deploy (exported to the deploy command as `BOLT_COSMATIC_TARGET`).
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

    /// Initialize a new bolt-cos-matic project (interactive setup wizard).
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
        /// Built-in name (see `bolt-cosmatic library list`).
        name: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum MaturityAction {
    /// Validate one Rumble delivery maturity claim JSON file.
    Validate {
        /// Path to a rumble.delivery_maturity.v0.1 JSON claim.
        claim: PathBuf,

        /// Print a machine-readable JSON report.
        #[arg(long)]
        json: bool,
    },

    /// Validate every maturity claim JSON file in a directory and print a summary.
    Report {
        /// Directory containing maturity claim JSON files.
        dir: PathBuf,

        /// Print a machine-readable JSON report.
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Subcommand)]
pub enum HandoffAction {
    /// Validate a handoff JSON payload and print findings.
    Validate {
        /// Path to the handoff JSON payload.
        payload: PathBuf,

        /// Print a machine-readable JSON report.
        #[arg(long)]
        json: bool,
    },

    /// Produce a planning-only dry-run report from a valid handoff payload.
    Plan {
        /// Path to the handoff JSON payload.
        payload: PathBuf,

        /// Required safety flag: planning only, no implementation execution.
        #[arg(long)]
        dry_run: bool,

        /// Optional Wrench EvidenceReport v0.1 files projected as evidence refs.
        #[arg(long = "evidence-report")]
        evidence_reports: Vec<PathBuf>,

        /// Optional Gear ArtifactManifest files for Wrench evidence reports.
        #[arg(long = "evidence-manifest")]
        evidence_manifests: Vec<PathBuf>,

        /// Optional signed human approval v0.1 files projected as evidence refs.
        #[arg(long = "human-approval")]
        human_approvals: Vec<PathBuf>,

        /// Optional approval key registry used to resolve human approval public_key_ref values.
        #[arg(long = "approval-key-registry")]
        approval_key_registry: Option<PathBuf>,

        /// Print a machine-readable JSON report.
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Subcommand)]
pub enum InspectAction {
    /// Check that exceptions to Rust-core have ADR coverage.
    AdrRequired {
        /// Repository root to inspect.
        #[arg(long, default_value = ".")]
        root: PathBuf,

        /// Optional TOML policy. If omitted, the strict Rust-core default is used.
        #[arg(long)]
        policy: Option<PathBuf>,
    },

    /// Check that languages stay within their architectural ownership zones.
    LanguageOwnership {
        /// Repository root to inspect.
        #[arg(long, default_value = ".")]
        root: PathBuf,

        /// Optional TOML policy. If omitted, the strict Rust-core default is used.
        #[arg(long)]
        policy: Option<PathBuf>,
    },

    /// Check that frontend source is TypeScript-only and Bun-toolchain clean.
    FrontendStrict {
        /// Repository root to inspect.
        #[arg(long, default_value = ".")]
        root: PathBuf,

        /// Optional TOML policy. If omitted, the strict Rust-core default is used.
        #[arg(long)]
        policy: Option<PathBuf>,
    },

    /// Check that shell remains temporary glue, not durable automation.
    ShellDebt {
        /// Repository root to inspect.
        #[arg(long, default_value = ".")]
        root: PathBuf,

        /// Optional TOML policy. If omitted, the strict Rust-core default is used.
        #[arg(long)]
        policy: Option<PathBuf>,
    },
}

#[derive(Debug, Subcommand)]
pub enum StackAction {
    /// Summarize repository state without modifying it.
    ProjectStatus {
        /// Repository root to inspect.
        #[arg(long, default_value = ".")]
        root: PathBuf,

        /// Print a machine-readable JSON report.
        #[arg(long)]
        json: bool,
    },

    /// Detect stack components and suggested local gates.
    Detect {
        /// Repository root to inspect.
        #[arg(long, default_value = ".")]
        root: PathBuf,

        /// Print a machine-readable JSON report.
        #[arg(long)]
        json: bool,
    },

    /// Score the detected stack against safety, quality, performance, completeness, and sovereignty.
    Scorecard {
        /// Repository root to inspect.
        #[arg(long, default_value = ".")]
        root: PathBuf,

        /// Print a machine-readable JSON report.
        #[arg(long)]
        json: bool,
    },

    /// Audit local dependency manifests for license, vulnerability, and sovereignty risk signals.
    DependencyAudit {
        /// Repository root to inspect.
        #[arg(long, default_value = ".")]
        root: PathBuf,

        /// Print a machine-readable JSON report.
        #[arg(long)]
        json: bool,
    },

    /// Run explicit local smoke commands after safety screening.
    LocalSmoke {
        /// Repository root where commands run.
        #[arg(long, default_value = ".")]
        root: PathBuf,

        /// Command to run. Repeat for multiple commands. Refuses dangerous/provisioning commands.
        #[arg(long = "cmd")]
        commands: Vec<String>,

        /// Per-command timeout in seconds.
        #[arg(long, default_value_t = 120)]
        timeout_seconds: u64,

        /// Print a machine-readable JSON report.
        #[arg(long)]
        json: bool,
    },

    /// Static local-only PostgreSQL security checks for migrations/schemas/fixtures.
    #[command(name = "db_security_check", visible_alias = "db-security-check")]
    DbSecurityCheck {
        /// Repository root to inspect.
        #[arg(long, default_value = ".")]
        root: PathBuf,

        /// Optional database URL signal. Value is never printed; connections are refused unless explicitly allowed.
        #[arg(long)]
        database_url: Option<String>,

        /// Explicitly acknowledge that a DB connection was requested. This command still performs static checks only.
        #[arg(long)]
        allow_db_connection: bool,

        /// Print a machine-readable JSON report.
        #[arg(long)]
        json: bool,
    },

    /// Generate a Markdown ADR draft from an accepted decision reference; never accepts the ADR automatically.
    #[command(name = "adr_generate", visible_alias = "adr-generate")]
    AdrGenerate {
        /// ADR title.
        #[arg(long)]
        title: Option<String>,

        /// Reference to the already accepted decision that motivates the ADR.
        #[arg(long)]
        accepted_decision_ref: Option<String>,

        /// Context section text.
        #[arg(long)]
        context: Option<String>,

        /// Decision section text.
        #[arg(long)]
        decision: Option<String>,

        /// Consequence bullet. Repeat for multiple consequences.
        #[arg(long = "consequence")]
        consequences: Vec<String>,

        /// Reversibility section text.
        #[arg(long)]
        reversibility: Option<String>,

        /// Print a machine-readable JSON report instead of Markdown.
        #[arg(long)]
        json: bool,
    },

    /// Verify deployment prerequisites without executing, pushing, provisioning, or applying changes.
    #[command(name = "deploy_dry_run", visible_alias = "deploy-dry-run")]
    DeployDryRun {
        /// Repository root to inspect.
        #[arg(long, default_value = ".")]
        root: PathBuf,

        /// Command to classify as a deployment prerequisite. Repeat for multiple commands. Never executed.
        #[arg(long = "cmd")]
        commands: Vec<String>,

        /// Print a machine-readable JSON report.
        #[arg(long)]
        json: bool,
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
