//! Local-only stack validation helpers.
//!
//! These functions inspect repository files and manifests, then return
//! machine-readable evidence. They do not install dependencies, call providers,
//! or create remote resources.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::{Error, Result};

const VERSION: &str = "0.1";
const MODE: &str = "local_only";
const MAX_SCAN_FILES: usize = 10_000;
const MAX_SCAN_DEPTH: usize = 12;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StackFinding {
    pub axis: String,
    pub severity: StackSeverity,
    pub message: String,
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StackSeverity {
    Pass,
    Warn,
    Fail,
}

impl StackSeverity {
    pub fn label(self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::Warn => "WARN",
            Self::Fail => "FAIL",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectStatusReport {
    pub tool: String,
    pub version: String,
    pub mode: String,
    pub target: String,
    pub git: GitStatus,
    pub files: FileSummary,
    pub detected_scripts: Vec<String>,
    pub findings: Vec<StackFinding>,
    pub next_actions: Vec<String>,
    pub redactions_applied: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitStatus {
    pub is_repo: bool,
    pub branch: Option<String>,
    pub dirty_entries: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSummary {
    pub tracked_signals: Vec<String>,
    pub untracked_signals: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StackDetectReport {
    pub tool: String,
    pub version: String,
    pub mode: String,
    pub target: String,
    pub components: Vec<DetectedComponent>,
    pub suggested_commands: Vec<String>,
    pub missing_gates: Vec<String>,
    pub findings: Vec<StackFinding>,
    pub redactions_applied: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedComponent {
    pub kind: String,
    pub name: String,
    pub confidence: String,
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyAuditReport {
    pub tool: String,
    pub version: String,
    pub mode: String,
    pub target: String,
    pub manifests: Vec<String>,
    pub findings: Vec<StackFinding>,
    pub waiver_candidates: Vec<String>,
    pub redactions_applied: bool,
}

impl DependencyAuditReport {
    pub fn has_failures(&self) -> bool {
        self.findings
            .iter()
            .any(|finding| finding.severity == StackSeverity::Fail)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StackScorecardReport {
    pub tool: String,
    pub version: String,
    pub mode: String,
    pub target: String,
    pub decision: StackDecision,
    pub axes: Vec<ScoreAxis>,
    pub findings: Vec<StackFinding>,
    pub next_actions: Vec<String>,
    pub redactions_applied: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum StackDecision {
    Go,
    ConditionalGo,
    SpikeLocal,
    Wait,
    NoGo,
}

impl std::fmt::Display for StackDecision {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            Self::Go => "GO",
            Self::ConditionalGo => "CONDITIONAL_GO",
            Self::SpikeLocal => "SPIKE_LOCAL",
            Self::Wait => "WAIT",
            Self::NoGo => "NO_GO",
        };
        formatter.write_str(label)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoreAxis {
    pub axis: String,
    pub status: StackSeverity,
    pub evidence: Vec<String>,
    pub missing_evidence: Vec<String>,
}

pub fn project_status(root: &Path) -> Result<ProjectStatusReport> {
    let files = collect_files(root)?;
    let git = git_status(root);
    let detected_scripts = detect_scripts(root)?;
    let mut findings = Vec::new();
    let mut next_actions = Vec::new();

    if git.is_repo && !git.dirty_entries.is_empty() {
        findings.push(StackFinding {
            axis: "quality".to_string(),
            severity: StackSeverity::Warn,
            message: "repository has local changes; distinguish pre-existing work before editing"
                .to_string(),
            evidence: git.dirty_entries.iter().take(10).cloned().collect(),
        });
    }

    if detected_scripts.is_empty() {
        findings.push(StackFinding {
            axis: "completeness".to_string(),
            severity: StackSeverity::Warn,
            message: "no local verification scripts detected".to_string(),
            evidence: Vec::new(),
        });
        next_actions.push("run stack_detect to identify manual verification commands".to_string());
    } else {
        next_actions.push("run stack_detect, then stack_scorecard with local evidence".to_string());
    }

    Ok(ProjectStatusReport {
        tool: "project_status".to_string(),
        version: VERSION.to_string(),
        mode: MODE.to_string(),
        target: display_path(root),
        git,
        files: FileSummary::from(&files),
        detected_scripts,
        findings,
        next_actions,
        redactions_applied: true,
    })
}

pub fn stack_detect(root: &Path) -> Result<StackDetectReport> {
    let files = collect_files(root)?;
    let mut components = Vec::new();
    let mut suggested_commands = Vec::new();
    let mut missing_gates = Vec::new();
    let mut findings = Vec::new();

    if has_file(&files.all, "Cargo.toml") {
        components.push(component("language", "Rust", "high", &["Cargo.toml"]));
        suggested_commands.extend([
            "cargo fmt --all -- --check".to_string(),
            "cargo clippy --workspace --all-targets -- -D warnings".to_string(),
            "cargo test --workspace --all-targets".to_string(),
        ]);
        if !has_file(&files.all, "deny.toml") {
            missing_gates.push("cargo deny license/advisory policy".to_string());
        }
    }

    if has_file(&files.all, "package.json") {
        components.push(component(
            "toolchain",
            "Node/Bun web",
            "medium",
            &["package.json"],
        ));
        if has_file(&files.all, "bun.lock") || has_file(&files.all, "bun.lockb") {
            components.push(component(
                "toolchain",
                "Bun",
                "high",
                &["bun.lock or bun.lockb"],
            ));
            suggested_commands.push("bun install --frozen-lockfile".to_string());
        }
        suggested_commands.extend(["bun run check".to_string(), "bun run build".to_string()]);
    }

    if any_file_ends_with(&files.all, "astro.config.mjs")
        || any_file_ends_with(&files.all, "astro.config.ts")
    {
        components.push(component(
            "frontend",
            "Astro static site",
            "high",
            &["astro config"],
        ));
    }

    if any_path_contains(&files.all, "playwright") {
        components.push(component(
            "test",
            "Playwright",
            "medium",
            &["playwright path/config"],
        ));
        suggested_commands.push("bun run test -- --project=chromium".to_string());
    }

    if any_path_contains(&files.all, "migrations")
        || file_text_contains(root, "Cargo.toml", "sqlx")?
    {
        components.push(component(
            "database",
            "SQLx/PostgreSQL-ready",
            "medium",
            &["migrations or sqlx"],
        ));
        missing_gates.push("local migration smoke or SQLx check evidence".to_string());
    }

    if file_text_contains(root, "Cargo.toml", "dioxus")? {
        components.push(component(
            "frontend",
            "Dioxus/PWA candidate",
            "medium",
            &["Cargo.toml mentions dioxus"],
        ));
        suggested_commands.push("cargo check --target wasm32-unknown-unknown".to_string());
        findings.push(StackFinding {
            axis: "completeness".to_string(),
            severity: StackSeverity::Warn,
            message: "Dioxus/PWA remains a local spike until mobile smoke and token-boundary evidence exist".to_string(),
            evidence: vec!["stack-validation local-only decision matrix".to_string()],
        });
    }

    if !any_path_contains(&files.all, ".github/workflows")
        && !any_path_contains(&files.all, ".gitlab-ci")
    {
        missing_gates.push("CI workflow evidence".to_string());
    }

    Ok(StackDetectReport {
        tool: "stack_detect".to_string(),
        version: VERSION.to_string(),
        mode: MODE.to_string(),
        target: display_path(root),
        components,
        suggested_commands: dedup(suggested_commands),
        missing_gates: dedup(missing_gates),
        findings,
        redactions_applied: true,
    })
}

pub fn dependency_audit(root: &Path) -> Result<DependencyAuditReport> {
    let files = collect_files(root)?;
    let manifests: Vec<String> = files
        .all
        .iter()
        .filter(|path| is_manifest(path))
        .cloned()
        .collect();
    let mut findings = Vec::new();
    let mut waiver_candidates = Vec::new();

    for manifest in &manifests {
        let text = read_optional(root, manifest)?
            .unwrap_or_default()
            .to_lowercase();
        for (needle, message, severity) in risky_patterns() {
            if text.contains(needle) {
                findings.push(StackFinding {
                    axis: "sovereignty".to_string(),
                    severity,
                    message: message.to_string(),
                    evidence: vec![manifest.clone()],
                });
                if severity != StackSeverity::Pass {
                    waiver_candidates.push(format!("{manifest}: {message}"));
                }
            }
        }
    }

    if manifests.iter().any(|path| path.ends_with("Cargo.toml"))
        && !files.all.iter().any(|path| path.ends_with("deny.toml"))
    {
        findings.push(StackFinding {
            axis: "sovereignty".to_string(),
            severity: StackSeverity::Warn,
            message: "Rust project has no deny.toml license/advisory policy".to_string(),
            evidence: vec!["Cargo.toml".to_string()],
        });
    }

    if findings.is_empty() {
        findings.push(StackFinding {
            axis: "sovereignty".to_string(),
            severity: StackSeverity::Pass,
            message: "no forbidden dependency pattern detected in local manifests".to_string(),
            evidence: manifests.clone(),
        });
    }

    Ok(DependencyAuditReport {
        tool: "dependency_audit".to_string(),
        version: VERSION.to_string(),
        mode: MODE.to_string(),
        target: display_path(root),
        manifests,
        findings,
        waiver_candidates: dedup(waiver_candidates),
        redactions_applied: true,
    })
}

pub fn stack_scorecard(root: &Path) -> Result<StackScorecardReport> {
    let detect = stack_detect(root)?;
    let audit = dependency_audit(root)?;
    let mut axes = Vec::new();
    let mut findings = Vec::new();
    findings.extend(detect.findings.clone());
    findings.extend(audit.findings.clone());

    let security_missing = missing(
        &detect,
        &["cargo audit", "secret scan", "auth allow/deny fixtures"],
    );
    axes.push(axis_from_missing("security", security_missing));

    let quality_missing = missing(&detect, &["format check", "lint", "tests"]);
    axes.push(axis_from_missing("quality", quality_missing));

    axes.push(ScoreAxis {
        axis: "performance".to_string(),
        status: StackSeverity::Warn,
        evidence: Vec::new(),
        missing_evidence: vec!["no measured hot-path evidence yet".to_string()],
    });

    let completeness_missing = if detect.missing_gates.is_empty() {
        Vec::new()
    } else {
        detect.missing_gates.clone()
    };
    axes.push(axis_from_missing("completeness", completeness_missing));

    let sovereignty_status = if audit.has_failures() {
        StackSeverity::Fail
    } else if audit
        .findings
        .iter()
        .any(|finding| finding.severity == StackSeverity::Warn)
    {
        StackSeverity::Warn
    } else {
        StackSeverity::Pass
    };
    axes.push(ScoreAxis {
        axis: "sovereignty".to_string(),
        status: sovereignty_status,
        evidence: audit
            .findings
            .iter()
            .flat_map(|finding| finding.evidence.clone())
            .collect(),
        missing_evidence: if sovereignty_status == StackSeverity::Pass {
            Vec::new()
        } else {
            vec!["resolve dependency/provider/license findings".to_string()]
        },
    });

    let decision = decide(&axes, &detect);
    let next_actions = next_actions_for(decision, &axes);

    Ok(StackScorecardReport {
        tool: "stack_scorecard".to_string(),
        version: VERSION.to_string(),
        mode: MODE.to_string(),
        target: display_path(root),
        decision,
        axes,
        findings,
        next_actions,
        redactions_applied: true,
    })
}

fn decide(axes: &[ScoreAxis], detect: &StackDetectReport) -> StackDecision {
    if axes.iter().any(|axis| axis.status == StackSeverity::Fail) {
        return StackDecision::NoGo;
    }
    let has_rag = detect
        .components
        .iter()
        .any(|component| component.name.to_lowercase().contains("pgvector"))
        || detect
            .findings
            .iter()
            .any(|finding| finding.message.to_lowercase().contains("rag"));
    let has_dioxus = detect
        .components
        .iter()
        .any(|component| component.name.to_lowercase().contains("dioxus"));
    if has_rag || has_dioxus {
        return StackDecision::SpikeLocal;
    }
    if axes.iter().any(|axis| axis.status == StackSeverity::Warn) {
        StackDecision::ConditionalGo
    } else {
        StackDecision::Go
    }
}

fn next_actions_for(decision: StackDecision, axes: &[ScoreAxis]) -> Vec<String> {
    let mut actions = Vec::new();
    match decision {
        StackDecision::Go => actions.push("keep local gates green and record evidence".to_string()),
        StackDecision::ConditionalGo => {
            actions.push("fill missing evidence before implementation".to_string())
        }
        StackDecision::SpikeLocal => {
            actions.push("run a fixture-first local spike with explicit acceptance".to_string())
        }
        StackDecision::Wait => actions.push("wait for concrete product pressure".to_string()),
        StackDecision::NoGo => {
            actions.push("resolve blocking security or sovereignty findings".to_string())
        }
    }
    for axis in axes {
        for missing in &axis.missing_evidence {
            actions.push(format!("{}: add {missing}", axis.axis));
        }
    }
    dedup(actions)
}

fn axis_from_missing(axis: &str, missing_evidence: Vec<String>) -> ScoreAxis {
    ScoreAxis {
        axis: axis.to_string(),
        status: if missing_evidence.is_empty() {
            StackSeverity::Pass
        } else {
            StackSeverity::Warn
        },
        evidence: Vec::new(),
        missing_evidence,
    }
}

fn missing(detect: &StackDetectReport, checks: &[&str]) -> Vec<String> {
    checks
        .iter()
        .filter(|check| !has_command_evidence(detect, check))
        .map(|check| (*check).to_string())
        .collect()
}

fn has_command_evidence(detect: &StackDetectReport, check: &str) -> bool {
    let needles: Vec<&str> = match check {
        "format check" => vec!["fmt", "format"],
        "lint" => vec!["clippy", "lint"],
        "tests" => vec!["test"],
        "cargo audit" => vec!["cargo audit"],
        "secret scan" => vec!["gitleaks", "secret"],
        "auth allow/deny fixtures" => vec!["allow/deny", "auth fixture"],
        other => vec![other],
    };
    detect.suggested_commands.iter().any(|command| {
        let command = command.to_lowercase();
        needles.iter().any(|needle| command.contains(needle))
    })
}

fn component(kind: &str, name: &str, confidence: &str, evidence: &[&str]) -> DetectedComponent {
    DetectedComponent {
        kind: kind.to_string(),
        name: name.to_string(),
        confidence: confidence.to_string(),
        evidence: evidence.iter().map(|item| (*item).to_string()).collect(),
    }
}

#[derive(Debug)]
struct CollectedFiles {
    all: Vec<String>,
}

fn collect_files(root: &Path) -> Result<CollectedFiles> {
    let mut all = Vec::new();
    collect_files_inner(root, root, 0, &mut all)?;
    all.sort();
    Ok(CollectedFiles { all })
}

fn collect_files_inner(root: &Path, dir: &Path, depth: usize, all: &mut Vec<String>) -> Result<()> {
    if depth > MAX_SCAN_DEPTH || all.len() >= MAX_SCAN_FILES {
        return Ok(());
    }
    let read_dir = fs::read_dir(dir).map_err(|source| Error::Io {
        path: display_path(dir),
        source,
    })?;
    for entry in read_dir {
        let entry = entry.map_err(|source| Error::Io {
            path: display_path(dir),
            source,
        })?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if should_skip(&name) {
            continue;
        }
        let file_type = entry.file_type().map_err(|source| Error::Io {
            path: display_path(&path),
            source,
        })?;
        if file_type.is_dir() {
            collect_files_inner(root, &path, depth + 1, all)?;
        } else if file_type.is_file() && all.len() < MAX_SCAN_FILES {
            all.push(relative_path(root, &path));
        }
    }
    Ok(())
}

fn should_skip(name: &str) -> bool {
    matches!(
        name,
        ".git" | "target" | "node_modules" | ".worktrees" | "dist" | ".next" | ".astro"
    )
}

fn git_status(root: &Path) -> GitStatus {
    let branch_output = Command::new("git")
        .args(["-C"])
        .arg(root)
        .args(["branch", "--show-current"])
        .output();
    let Ok(branch_output) = branch_output else {
        return GitStatus {
            is_repo: false,
            branch: None,
            dirty_entries: Vec::new(),
        };
    };
    if !branch_output.status.success() {
        return GitStatus {
            is_repo: false,
            branch: None,
            dirty_entries: Vec::new(),
        };
    }
    let branch = String::from_utf8_lossy(&branch_output.stdout)
        .trim()
        .to_string();
    let dirty_entries = Command::new("git")
        .args(["-C"])
        .arg(root)
        .args(["status", "--porcelain"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| {
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    GitStatus {
        is_repo: true,
        branch: if branch.is_empty() {
            None
        } else {
            Some(branch)
        },
        dirty_entries,
    }
}

fn detect_scripts(root: &Path) -> Result<Vec<String>> {
    let mut scripts = Vec::new();
    if root.join("Cargo.toml").exists() {
        scripts.extend([
            "cargo fmt --all -- --check".to_string(),
            "cargo clippy --workspace --all-targets -- -D warnings".to_string(),
            "cargo test --workspace --all-targets".to_string(),
        ]);
    }
    if root.join("package.json").exists() {
        let package = read_optional(root, "package.json")?.unwrap_or_default();
        for script in ["check", "build", "test"] {
            if package.contains(&format!("\"{script}\"")) {
                scripts.push(format!("bun run {script}"));
            }
        }
    }
    Ok(dedup(scripts))
}

fn has_file(files: &[String], path: &str) -> bool {
    files
        .iter()
        .any(|file| file == path || file.ends_with(path))
}

fn any_file_ends_with(files: &[String], suffix: &str) -> bool {
    files.iter().any(|file| file.ends_with(suffix))
}

fn any_path_contains(files: &[String], needle: &str) -> bool {
    files.iter().any(|file| file.contains(needle))
}

fn file_text_contains(root: &Path, path: &str, needle: &str) -> Result<bool> {
    Ok(read_optional(root, path)?.is_some_and(|text| text.to_lowercase().contains(needle)))
}

fn read_optional(root: &Path, relative: impl AsRef<Path>) -> Result<Option<String>> {
    let path = root.join(relative.as_ref());
    if !path.exists() {
        return Ok(None);
    }
    fs::read_to_string(&path)
        .map(Some)
        .map_err(|source| Error::Io {
            path: display_path(&path),
            source,
        })
}

fn is_manifest(path: &str) -> bool {
    path.ends_with("Cargo.toml")
        || path.ends_with("package.json")
        || path.ends_with("pyproject.toml")
        || path.ends_with("requirements.txt")
        || path.ends_with("deno.json")
}

fn risky_patterns() -> Vec<(&'static str, &'static str, StackSeverity)> {
    vec![
        (
            "agpl",
            "AGPL license pattern requires rejection or explicit legal waiver",
            StackSeverity::Fail,
        ),
        (
            "sspl",
            "SSPL/source-available license pattern is forbidden by default",
            StackSeverity::Fail,
        ),
        (
            "business source license",
            "BSL/source-available license pattern is forbidden by default",
            StackSeverity::Fail,
        ),
        (
            "aws-sdk",
            "AWS SDK in a manifest is a sovereignty risk for core paths",
            StackSeverity::Fail,
        ),
        (
            "@aws-sdk",
            "AWS SDK in a manifest is a sovereignty risk for core paths",
            StackSeverity::Fail,
        ),
        (
            "firebase",
            "Firebase dependency is forbidden by default for core systems",
            StackSeverity::Fail,
        ),
        (
            "auth0",
            "Auth0 dependency is forbidden by default for core systems",
            StackSeverity::Fail,
        ),
        (
            "openai",
            "OpenAI runtime dependency requires explicit waiver/provider policy",
            StackSeverity::Warn,
        ),
        (
            "anthropic",
            "Anthropic runtime dependency requires explicit waiver/provider policy",
            StackSeverity::Warn,
        ),
        (
            "native-tls",
            "native-tls is discouraged in portable Rust paths; prefer rustls",
            StackSeverity::Warn,
        ),
        (
            "openssl",
            "OpenSSL dependency is discouraged in portable Rust paths; prefer Rust-native TLS",
            StackSeverity::Warn,
        ),
        (
            "google-font",
            "remote Google Fonts/tracking must be avoided; self-host fonts",
            StackSeverity::Warn,
        ),
    ]
}

fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn display_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn dedup(mut items: Vec<String>) -> Vec<String> {
    items.sort();
    items.dedup();
    items
}

impl From<Vec<String>> for FileSummary {
    fn from(all: Vec<String>) -> Self {
        Self {
            tracked_signals: all,
            untracked_signals: Vec::new(),
        }
    }
}

impl From<CollectedFiles> for FileSummary {
    fn from(files: CollectedFiles) -> Self {
        Self {
            tracked_signals: files.all,
            untracked_signals: Vec::new(),
        }
    }
}

impl From<&CollectedFiles> for FileSummary {
    fn from(files: &CollectedFiles) -> Self {
        let tracked_signals = files
            .all
            .iter()
            .filter(|path| is_signal(path))
            .cloned()
            .collect();
        Self {
            tracked_signals,
            untracked_signals: Vec::new(),
        }
    }
}

fn is_signal(path: &str) -> bool {
    matches!(
        PathBuf::from(path)
            .file_name()
            .and_then(|name| name.to_str()),
        Some(
            "Cargo.toml"
                | "package.json"
                | "bun.lock"
                | "bun.lockb"
                | "deny.toml"
                | "README.md"
                | "AGENTS.md"
                | "SECURITY.md"
                | "astro.config.mjs"
                | "astro.config.ts"
        )
    ) || path.contains("migrations")
        || path.contains(".github/workflows")
}
