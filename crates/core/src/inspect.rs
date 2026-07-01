//! Deterministic repository inspections.
//!
//! These checks are intentionally local, filesystem-only, and policy-driven.
//! They are the first executable form of ADR: rust-core-doctrine-hardening.
//! The module lives in `bolt-cos-matic` while `wrench-inspect` is still a
//! skeleton, but the API is narrow so it can be extracted later.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::{Error, Result};

/// Language ownership policy loaded from TOML.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct LanguageOwnershipPolicy {
    #[serde(default)]
    pub language_ownership: LanguageOwnership,
}

/// The root section.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
pub struct LanguageOwnership {
    #[serde(default)]
    pub zones: Vec<LanguageZone>,
}

/// A path zone with extension allow/deny rules.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct LanguageZone {
    pub name: String,
    /// Repo-relative directory prefixes, e.g. `crates/`, `src/`, `web/`.
    #[serde(default)]
    pub paths: Vec<String>,
    /// Extensions allowed in this zone, without leading dots.
    #[serde(default)]
    pub allow_extensions: Vec<String>,
    /// Extensions denied in this zone, without leading dots. Deny wins over allow.
    #[serde(default)]
    pub deny_extensions: Vec<String>,
}

/// One language ownership violation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanguageFinding {
    pub path: String,
    pub zone: String,
    pub extension: String,
    pub reason: String,
}

/// Inspection report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanguageOwnershipReport {
    pub checked_files: usize,
    pub findings: Vec<LanguageFinding>,
}

impl LanguageOwnershipReport {
    pub fn is_clean(&self) -> bool {
        self.findings.is_empty()
    }
}

/// Frontend strictness policy loaded from TOML.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct FrontendStrictPolicy {
    #[serde(default)]
    pub frontend_strict: FrontendStrict,
}

/// Frontend TypeScript-only rules.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct FrontendStrict {
    #[serde(default)]
    pub paths: Vec<String>,
    #[serde(default)]
    pub forbidden_js_extensions: Vec<String>,
    #[serde(default)]
    pub forbidden_locks: Vec<String>,
    #[serde(default)]
    pub require_bun_lock: bool,
    #[serde(default = "default_true")]
    pub require_tsconfig_strict: bool,
}

impl Default for FrontendStrict {
    fn default() -> Self {
        Self {
            paths: vec!["web/".to_string(), "apps/".to_string()],
            forbidden_js_extensions: vec![
                "js".to_string(),
                "jsx".to_string(),
                "mjs".to_string(),
                "cjs".to_string(),
            ],
            forbidden_locks: vec![
                "package-lock.json".to_string(),
                "yarn.lock".to_string(),
                "pnpm-lock.yaml".to_string(),
            ],
            require_bun_lock: true,
            require_tsconfig_strict: true,
        }
    }
}

/// One frontend strictness finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrontendStrictFinding {
    pub path: String,
    pub reason: String,
}

/// Frontend strictness report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrontendStrictReport {
    pub checked_files: usize,
    pub findings: Vec<FrontendStrictFinding>,
}

impl FrontendStrictReport {
    pub fn is_clean(&self) -> bool {
        self.findings.is_empty()
    }
}

fn default_true() -> bool {
    true
}

/// Shell debt policy loaded from TOML.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ShellDebtPolicy {
    #[serde(default)]
    pub shell_debt: ShellDebt,
}

/// Shell debt thresholds and path rules.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ShellDebt {
    #[serde(default = "default_shell_line_limit")]
    pub max_lines_without_adr: usize,
    #[serde(default)]
    pub allowed_paths: Vec<String>,
    #[serde(default)]
    pub forbidden_paths: Vec<String>,
    #[serde(default)]
    pub exception_paths: Vec<String>,
}

impl Default for ShellDebt {
    fn default() -> Self {
        Self {
            max_lines_without_adr: default_shell_line_limit(),
            allowed_paths: vec!["scripts/".to_string()],
            forbidden_paths: vec![
                "crates/".to_string(),
                "src/".to_string(),
                "release/".to_string(),
                "deploy/".to_string(),
            ],
            exception_paths: Vec::new(),
        }
    }
}

/// One shell debt finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellDebtFinding {
    pub path: String,
    pub lines: usize,
    pub reason: String,
}

/// Shell debt report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellDebtReport {
    pub checked_scripts: usize,
    pub findings: Vec<ShellDebtFinding>,
}

impl ShellDebtReport {
    pub fn is_clean(&self) -> bool {
        self.findings.is_empty()
    }
}

fn default_shell_line_limit() -> usize {
    50
}

/// ADR-required policy: exceptions to Rust-core need written decisions.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AdrRequiredPolicy {
    #[serde(default)]
    pub adr_required: AdrRequired,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AdrRequired {
    #[serde(default = "default_adr_dir")]
    pub adr_dir: String,
    #[serde(default = "default_true")]
    pub require_for_zig: bool,
    #[serde(default = "default_true")]
    pub require_for_backend_ts: bool,
    #[serde(default = "default_true")]
    pub require_for_native_deps: bool,
    #[serde(default = "default_true")]
    pub require_for_shell_debt: bool,
    #[serde(default)]
    pub native_deps: Vec<String>,
    #[serde(default)]
    pub backend_ts_markers: Vec<String>,
    #[serde(default)]
    pub backend_ts_files: Vec<String>,
    #[serde(default)]
    pub exempt_paths: Vec<String>,
    #[serde(default)]
    pub release_paths: Vec<String>,
}

impl Default for AdrRequired {
    fn default() -> Self {
        Self {
            adr_dir: default_adr_dir(),
            require_for_zig: true,
            require_for_backend_ts: true,
            require_for_native_deps: true,
            require_for_shell_debt: true,
            native_deps: vec![
                "openssl".to_string(),
                "openssl-sys".to_string(),
                "native-tls".to_string(),
                "bindgen".to_string(),
                "cc".to_string(),
                "pkg-config".to_string(),
                "cmake".to_string(),
                "libgit2-sys".to_string(),
            ],
            backend_ts_markers: vec![
                "Bun.serve".to_string(),
                "hono".to_string(),
                "elysia".to_string(),
                "express".to_string(),
                "fastify".to_string(),
                "drizzle".to_string(),
                "@prisma/client".to_string(),
                "postgres".to_string(),
                "bullmq".to_string(),
                "@aws-sdk/client-s3".to_string(),
                "@biscuit-auth".to_string(),
            ],
            backend_ts_files: vec![
                "server.ts".to_string(),
                "api.ts".to_string(),
                "routes.ts".to_string(),
            ],
            exempt_paths: vec!["web/".to_string(), "apps/".to_string()],
            release_paths: vec![
                "release/".to_string(),
                "deploy/".to_string(),
                "scripts/deploy".to_string(),
            ],
        }
    }
}

fn default_adr_dir() -> String {
    "docs/adr".to_string()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdrRequiredFinding {
    pub path: String,
    pub trigger: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdrRequiredReport {
    pub checked_files: usize,
    pub findings: Vec<AdrRequiredFinding>,
}

impl AdrRequiredReport {
    pub fn is_clean(&self) -> bool {
        self.findings.is_empty()
    }
}

/// A strict Rust-core default. Projects can pass a TOML policy when they need a
/// different directory layout.
pub fn default_language_ownership_policy() -> LanguageOwnershipPolicy {
    LanguageOwnershipPolicy {
        language_ownership: LanguageOwnership {
            zones: vec![
                LanguageZone {
                    name: "core".to_string(),
                    paths: vec!["crates/".to_string(), "src/".to_string()],
                    allow_extensions: vec![
                        "rs".to_string(),
                        "toml".to_string(),
                        "md".to_string(),
                        "sql".to_string(),
                        "json".to_string(),
                        "yml".to_string(),
                        "yaml".to_string(),
                    ],
                    deny_extensions: vec![
                        "ts".to_string(),
                        "tsx".to_string(),
                        "js".to_string(),
                        "jsx".to_string(),
                        "zig".to_string(),
                        "sh".to_string(),
                    ],
                },
                LanguageZone {
                    name: "web".to_string(),
                    paths: vec!["web/".to_string(), "apps/".to_string()],
                    allow_extensions: vec![
                        "ts".to_string(),
                        "tsx".to_string(),
                        "css".to_string(),
                        "html".to_string(),
                        "json".to_string(),
                        "md".to_string(),
                        "toml".to_string(),
                    ],
                    deny_extensions: vec![
                        "js".to_string(),
                        "jsx".to_string(),
                        "mjs".to_string(),
                        "cjs".to_string(),
                    ],
                },
            ],
        },
    }
}

/// Load a TOML language ownership policy from disk.
pub fn load_language_ownership_policy(path: &Path) -> Result<LanguageOwnershipPolicy> {
    let src = fs::read_to_string(path).map_err(|source| Error::Io {
        path: display_path(path),
        source,
    })?;
    toml::from_str(&src).map_err(|e| Error::InvalidPolicy {
        path: display_path(path),
        message: e.to_string(),
    })
}

/// Load a TOML frontend strictness policy from disk.
pub fn load_frontend_strict_policy(path: &Path) -> Result<FrontendStrictPolicy> {
    let src = fs::read_to_string(path).map_err(|source| Error::Io {
        path: display_path(path),
        source,
    })?;
    toml::from_str(&src).map_err(|e| Error::InvalidPolicy {
        path: display_path(path),
        message: e.to_string(),
    })
}

/// Default frontend strictness policy.
pub fn default_frontend_strict_policy() -> FrontendStrictPolicy {
    FrontendStrictPolicy {
        frontend_strict: FrontendStrict::default(),
    }
}

/// Load a TOML shell debt policy from disk.
pub fn load_shell_debt_policy(path: &Path) -> Result<ShellDebtPolicy> {
    let src = fs::read_to_string(path).map_err(|source| Error::Io {
        path: display_path(path),
        source,
    })?;
    toml::from_str(&src).map_err(|e| Error::InvalidPolicy {
        path: display_path(path),
        message: e.to_string(),
    })
}

/// Default shell debt policy.
pub fn default_shell_debt_policy() -> ShellDebtPolicy {
    ShellDebtPolicy {
        shell_debt: ShellDebt::default(),
    }
}

/// Load a TOML ADR-required policy from disk.
pub fn load_adr_required_policy(path: &Path) -> Result<AdrRequiredPolicy> {
    let src = fs::read_to_string(path).map_err(|source| Error::Io {
        path: display_path(path),
        source,
    })?;
    toml::from_str(&src).map_err(|e| Error::InvalidPolicy {
        path: display_path(path),
        message: e.to_string(),
    })
}

/// Default ADR-required policy.
pub fn default_adr_required_policy() -> AdrRequiredPolicy {
    AdrRequiredPolicy {
        adr_required: AdrRequired::default(),
    }
}

/// Inspect a repository root according to the policy.
pub fn inspect_language_ownership(
    root: &Path,
    policy: &LanguageOwnershipPolicy,
) -> Result<LanguageOwnershipReport> {
    let mut files = Vec::new();
    collect_files(root, root, &mut files)?;

    let mut findings = Vec::new();
    for relative in &files {
        let relative_str = relative.to_string_lossy().replace('\\', "/");
        let Some(zone) = policy.language_ownership.zones.iter().find(|z| {
            z.paths
                .iter()
                .any(|prefix| relative_str.starts_with(prefix))
        }) else {
            continue;
        };

        let extension = relative
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();

        if zone.deny_extensions.iter().any(|e| e == &extension) {
            findings.push(LanguageFinding {
                path: relative_str,
                zone: zone.name.clone(),
                extension,
                reason: "extension is denied in this zone".to_string(),
            });
            continue;
        }

        if !zone.allow_extensions.is_empty()
            && !extension.is_empty()
            && !zone.allow_extensions.iter().any(|e| e == &extension)
        {
            findings.push(LanguageFinding {
                path: relative_str,
                zone: zone.name.clone(),
                extension,
                reason: "extension is not allowed in this zone".to_string(),
            });
        }
    }

    Ok(LanguageOwnershipReport {
        checked_files: files.len(),
        findings,
    })
}

/// Inspect frontend zones for TypeScript-only strictness.
pub fn inspect_frontend_strict(
    root: &Path,
    policy: &FrontendStrictPolicy,
) -> Result<FrontendStrictReport> {
    let mut files = Vec::new();
    collect_files(root, root, &mut files)?;

    let frontend_files: Vec<PathBuf> = files
        .into_iter()
        .filter(|p| {
            let s = p.to_string_lossy().replace('\\', "/");
            policy
                .frontend_strict
                .paths
                .iter()
                .any(|prefix| s.starts_with(prefix))
        })
        .collect();

    let mut findings = Vec::new();
    for relative in &frontend_files {
        let relative_str = relative.to_string_lossy().replace('\\', "/");
        let file_name = relative.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let extension = relative
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();

        if policy
            .frontend_strict
            .forbidden_js_extensions
            .iter()
            .any(|e| e == &extension)
        {
            findings.push(FrontendStrictFinding {
                path: relative_str.clone(),
                reason: "JavaScript source is forbidden; use TypeScript".to_string(),
            });
        }

        if policy
            .frontend_strict
            .forbidden_locks
            .iter()
            .any(|lock| lock == file_name)
        {
            findings.push(FrontendStrictFinding {
                path: relative_str.clone(),
                reason: "non-Bun lockfile is forbidden in frontend zones".to_string(),
            });
        }

        if file_name == "tsconfig.json" && policy.frontend_strict.require_tsconfig_strict {
            let full_path = root.join(relative);
            let content = fs::read_to_string(&full_path).map_err(|source| Error::Io {
                path: display_path(&full_path),
                source,
            })?;
            if !content_contains_json_bool(&content, "strict", true)
                || content_contains_json_bool(&content, "allowJs", true)
            {
                findings.push(FrontendStrictFinding {
                    path: relative_str,
                    reason: "tsconfig.json must set strict=true and must not set allowJs=true"
                        .to_string(),
                });
            }
        }
    }

    if policy.frontend_strict.require_bun_lock {
        for prefix in &policy.frontend_strict.paths {
            let has_frontend_file = frontend_files
                .iter()
                .any(|p| p.to_string_lossy().replace('\\', "/").starts_with(prefix));
            if has_frontend_file && !root.join(prefix).join("bun.lock").exists() {
                findings.push(FrontendStrictFinding {
                    path: prefix.clone(),
                    reason: "frontend zone uses Bun policy but has no bun.lock".to_string(),
                });
            }
        }
    }

    Ok(FrontendStrictReport {
        checked_files: frontend_files.len(),
        findings,
    })
}

fn content_contains_json_bool(content: &str, key: &str, value: bool) -> bool {
    let expected = if value { "true" } else { "false" };
    content.lines().any(|line| {
        let line = line.split("//").next().unwrap_or(line);
        line.contains(&format!("\"{key}\"")) && line.contains(expected)
    })
}

/// Inspect shell scripts for durable automation debt.
pub fn inspect_shell_debt(root: &Path, policy: &ShellDebtPolicy) -> Result<ShellDebtReport> {
    let mut files = Vec::new();
    collect_files(root, root, &mut files)?;

    let scripts: Vec<PathBuf> = files
        .into_iter()
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("sh"))
        .collect();
    let mut findings = Vec::new();

    for relative in &scripts {
        let relative_str = relative.to_string_lossy().replace('\\', "/");
        if policy
            .shell_debt
            .exception_paths
            .iter()
            .any(|p| p == &relative_str)
        {
            continue;
        }

        let full_path = root.join(relative);
        let content = fs::read_to_string(&full_path).map_err(|source| Error::Io {
            path: display_path(&full_path),
            source,
        })?;
        let lines = content.lines().count();

        if policy
            .shell_debt
            .forbidden_paths
            .iter()
            .any(|prefix| relative_str.starts_with(prefix))
        {
            findings.push(ShellDebtFinding {
                path: relative_str,
                lines,
                reason: "shell is forbidden in this path".to_string(),
            });
            continue;
        }

        let is_allowed_path = policy
            .shell_debt
            .allowed_paths
            .iter()
            .any(|prefix| relative_str.starts_with(prefix));
        if !is_allowed_path {
            findings.push(ShellDebtFinding {
                path: relative_str,
                lines,
                reason: "shell script is outside allowed paths".to_string(),
            });
            continue;
        }

        if lines > policy.shell_debt.max_lines_without_adr && !has_adr_reference(&content) {
            findings.push(ShellDebtFinding {
                path: relative_str,
                lines,
                reason: format!(
                    "shell script exceeds {} lines without an ADR reference",
                    policy.shell_debt.max_lines_without_adr
                ),
            });
        }
    }

    Ok(ShellDebtReport {
        checked_scripts: scripts.len(),
        findings,
    })
}

/// Inspect exceptions that require an ADR.
pub fn inspect_adr_required(root: &Path, policy: &AdrRequiredPolicy) -> Result<AdrRequiredReport> {
    let mut files = Vec::new();
    collect_files(root, root, &mut files)?;
    let adr_corpus = read_adr_corpus(root, &policy.adr_required.adr_dir)?;
    let shell_policy = default_shell_debt_policy();
    let shell_report = inspect_shell_debt(root, &shell_policy)?;

    let mut findings = Vec::new();
    for relative in &files {
        let relative_str = relative.to_string_lossy().replace('\\', "/");
        if policy
            .adr_required
            .exempt_paths
            .iter()
            .any(|prefix| relative_str.starts_with(prefix))
        {
            continue;
        }

        let file_name = relative.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let extension = relative
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let full_path = root.join(relative);
        let content = if likely_text_file(relative) {
            fs::read_to_string(&full_path).unwrap_or_default()
        } else {
            String::new()
        };

        if policy.adr_required.require_for_zig
            && (extension == "zig" || file_name == "build.zig.zon")
            && !is_adr_covered(&relative_str, "zig", &content, &adr_corpus)
        {
            findings.push(AdrRequiredFinding {
                path: relative_str.clone(),
                trigger: "zig".to_string(),
                reason: "Zig/native source requires ADR coverage".to_string(),
            });
        }

        if policy.adr_required.require_for_backend_ts
            && matches!(extension.as_str(), "ts" | "tsx")
            && (policy
                .adr_required
                .backend_ts_files
                .iter()
                .any(|name| name == file_name)
                || policy
                    .adr_required
                    .backend_ts_markers
                    .iter()
                    .any(|marker| content.contains(marker)))
            && !is_adr_covered(&relative_str, "backend-ts", &content, &adr_corpus)
        {
            findings.push(AdrRequiredFinding {
                path: relative_str.clone(),
                trigger: "backend-ts".to_string(),
                reason: "TypeScript server/backend capability requires ADR coverage".to_string(),
            });
        }

        if policy.adr_required.require_for_native_deps
            && file_name == "Cargo.toml"
            && let Some(marker) = policy
                .adr_required
                .native_deps
                .iter()
                .find(|marker| cargo_toml_mentions_dep(&content, marker))
            && !is_adr_covered(&relative_str, marker, &content, &adr_corpus)
        {
            findings.push(AdrRequiredFinding {
                path: relative_str.clone(),
                trigger: marker.clone(),
                reason: "native Rust dependency requires ADR coverage".to_string(),
            });
        }

        if policy
            .adr_required
            .release_paths
            .iter()
            .any(|prefix| relative_str.starts_with(prefix))
            && !is_adr_covered(&relative_str, "release", &content, &adr_corpus)
        {
            findings.push(AdrRequiredFinding {
                path: relative_str.clone(),
                trigger: "release".to_string(),
                reason: "release/deploy path requires ADR coverage".to_string(),
            });
        }
    }

    if policy.adr_required.require_for_shell_debt {
        for shell in shell_report.findings {
            let content = fs::read_to_string(root.join(&shell.path)).unwrap_or_default();
            if !is_adr_covered(&shell.path, "shell", &content, &adr_corpus) {
                findings.push(AdrRequiredFinding {
                    path: shell.path,
                    trigger: "shell".to_string(),
                    reason: shell.reason,
                });
            }
        }
    }

    Ok(AdrRequiredReport {
        checked_files: files.len(),
        findings,
    })
}

fn read_adr_corpus(root: &Path, adr_dir: &str) -> Result<String> {
    let dir = root.join(adr_dir);
    if !dir.exists() {
        return Ok(String::new());
    }
    let mut files = Vec::new();
    collect_files(root, &dir, &mut files)?;
    let mut corpus = String::new();
    for relative in files {
        if relative.extension().and_then(|e| e.to_str()) == Some("md") {
            let path = root.join(relative);
            corpus.push_str(&fs::read_to_string(&path).map_err(|source| Error::Io {
                path: display_path(&path),
                source,
            })?);
            corpus.push('\n');
        }
    }
    Ok(corpus)
}

fn is_adr_covered(path: &str, trigger: &str, content: &str, adr_corpus: &str) -> bool {
    has_adr_reference(content) || adr_corpus.contains(path) || adr_corpus.contains(trigger)
}

fn cargo_toml_mentions_dep(content: &str, dep: &str) -> bool {
    content.lines().any(|line| {
        let line = line.trim();
        !line.starts_with('#')
            && (line.starts_with(&format!("{dep} =")) || line.starts_with(&format!("{dep}=")))
    })
}

fn likely_text_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some(
            "rs" | "toml"
                | "md"
                | "ts"
                | "tsx"
                | "js"
                | "jsx"
                | "zig"
                | "sh"
                | "yml"
                | "yaml"
                | "json"
        )
    )
}

fn has_adr_reference(content: &str) -> bool {
    content.contains("ADR:") || content.contains("ADR-")
}

fn collect_files(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(dir).map_err(|source| Error::Io {
        path: display_path(dir),
        source,
    })? {
        let entry = entry.map_err(|source| Error::Io {
            path: display_path(dir),
            source,
        })?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if should_skip_dir(&name) && path.is_dir() {
            continue;
        }
        if path.is_dir() {
            collect_files(root, &path, out)?;
        } else if path.is_file() {
            let relative = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
            out.push(relative);
        }
    }
    Ok(())
}

fn should_skip_dir(name: &str) -> bool {
    matches!(
        name,
        ".git" | "target" | "node_modules" | "dist" | "build" | ".next" | ".turbo" | ".claude"
    )
}

fn display_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_core_default_rejects_ts_in_crates() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("crates/core/src")).unwrap();
        fs::write(tmp.path().join("crates/core/src/lib.rs"), "").unwrap();
        fs::write(tmp.path().join("crates/core/src/server.ts"), "").unwrap();

        let report =
            inspect_language_ownership(tmp.path(), &default_language_ownership_policy()).unwrap();

        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].path, "crates/core/src/server.ts");
        assert_eq!(report.findings[0].zone, "core");
    }

    #[test]
    fn web_zone_allows_tsx() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("web/src")).unwrap();
        fs::write(tmp.path().join("web/src/App.tsx"), "").unwrap();

        let report =
            inspect_language_ownership(tmp.path(), &default_language_ownership_policy()).unwrap();

        assert!(report.is_clean());
        assert_eq!(report.checked_files, 1);
    }

    #[test]
    fn frontend_strict_rejects_js_source() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("web/src")).unwrap();
        fs::write(tmp.path().join("web/bun.lock"), "").unwrap();
        fs::write(
            tmp.path().join("web/tsconfig.json"),
            r#"{"compilerOptions":{"strict":true}}"#,
        )
        .unwrap();
        fs::write(tmp.path().join("web/src/app.js"), "").unwrap();

        let report =
            inspect_frontend_strict(tmp.path(), &default_frontend_strict_policy()).unwrap();

        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].path, "web/src/app.js");
    }

    #[test]
    fn frontend_strict_rejects_allow_js() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("web")).unwrap();
        fs::write(tmp.path().join("web/bun.lock"), "").unwrap();
        fs::write(
            tmp.path().join("web/tsconfig.json"),
            r#"{"compilerOptions":{"strict":true,"allowJs":true}}"#,
        )
        .unwrap();

        let report =
            inspect_frontend_strict(tmp.path(), &default_frontend_strict_policy()).unwrap();

        assert_eq!(report.findings.len(), 1);
        assert!(report.findings[0].reason.contains("strict=true"));
    }

    #[test]
    fn frontend_strict_accepts_ts_with_bun_lock_and_strict_tsconfig() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("web/src")).unwrap();
        fs::write(tmp.path().join("web/bun.lock"), "").unwrap();
        fs::write(
            tmp.path().join("web/tsconfig.json"),
            r#"{"compilerOptions":{"strict":true}}"#,
        )
        .unwrap();
        fs::write(tmp.path().join("web/src/App.tsx"), "").unwrap();

        let report =
            inspect_frontend_strict(tmp.path(), &default_frontend_strict_policy()).unwrap();

        assert!(report.is_clean());
    }

    #[test]
    fn adr_required_rejects_zig_without_coverage() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("native")).unwrap();
        fs::write(tmp.path().join("native/build.zig"), "pub fn main() void {}").unwrap();

        let report = inspect_adr_required(tmp.path(), &default_adr_required_policy()).unwrap();

        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].trigger, "zig");
    }

    #[test]
    fn adr_required_accepts_exception_with_adr_reference() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("native")).unwrap();
        fs::write(
            tmp.path().join("native/build.zig"),
            "// ADR: rust-core-doctrine-hardening\npub fn main() void {}",
        )
        .unwrap();

        let report = inspect_adr_required(tmp.path(), &default_adr_required_policy()).unwrap();

        assert!(report.is_clean());
    }

    #[test]
    fn adr_required_rejects_native_cargo_dependency_without_coverage() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("crates/core")).unwrap();
        fs::write(
            tmp.path().join("crates/core/Cargo.toml"),
            "[dependencies]\nopenssl = \"0.10\"",
        )
        .unwrap();

        let report = inspect_adr_required(tmp.path(), &default_adr_required_policy()).unwrap();

        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].trigger, "openssl");
    }

    #[test]
    fn shell_debt_rejects_long_script_without_adr() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("scripts")).unwrap();
        let long_script = std::iter::repeat_n("echo test", 51)
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(tmp.path().join("scripts/long.sh"), long_script).unwrap();

        let report = inspect_shell_debt(tmp.path(), &default_shell_debt_policy()).unwrap();

        assert_eq!(report.checked_scripts, 1);
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].path, "scripts/long.sh");
    }

    #[test]
    fn shell_debt_accepts_long_script_with_adr() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("scripts")).unwrap();
        let mut lines = vec!["# ADR: rust-core-doctrine-hardening"];
        lines.extend(std::iter::repeat_n("echo test", 51));
        fs::write(tmp.path().join("scripts/long.sh"), lines.join("\n")).unwrap();

        let report = inspect_shell_debt(tmp.path(), &default_shell_debt_policy()).unwrap();

        assert!(report.is_clean());
    }

    #[test]
    fn shell_debt_rejects_script_in_core() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("crates/core")).unwrap();
        fs::write(tmp.path().join("crates/core/build.sh"), "echo test").unwrap();

        let report = inspect_shell_debt(tmp.path(), &default_shell_debt_policy()).unwrap();

        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].reason, "shell is forbidden in this path");
    }

    #[test]
    fn custom_policy_rejects_zig_in_native_zone() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("native")).unwrap();
        fs::write(tmp.path().join("native/build.zig"), "").unwrap();
        let policy = LanguageOwnershipPolicy {
            language_ownership: LanguageOwnership {
                zones: vec![LanguageZone {
                    name: "native".to_string(),
                    paths: vec!["native/".to_string()],
                    allow_extensions: vec!["rs".to_string()],
                    deny_extensions: vec!["zig".to_string()],
                }],
            },
        };

        let report = inspect_language_ownership(tmp.path(), &policy).unwrap();

        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].extension, "zig");
    }
}
