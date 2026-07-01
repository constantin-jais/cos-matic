//! Read-only validation/reporting for Rumble delivery maturity claims.
//!
//! P0 intentionally does not execute evidence commands and does not promote
//! levels. It parses the claim, applies semantic guards, and reports blockers.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{Error, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaturityClaim {
    pub format: String,
    pub project: Project,
    pub claimed_at: String,
    pub current_level: Level,
    pub target_level: Level,
    #[serde(default)]
    pub next_level: Option<Level>,
    #[serde(default)]
    pub promotion_candidate: Option<PromotionCandidate>,
    pub axes: Axes,
    pub platform_readiness: PlatformReadiness,
    pub evidence: Vec<EvidenceRef>,
    pub learning_yield: Vec<LearningYield>,
    #[serde(default)]
    pub extraction_pressure: Vec<ExtractionPressure>,
    #[serde(default)]
    pub open_questions: Vec<String>,
    #[serde(default)]
    pub risk_notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub name: String,
    pub layer: String,
    pub role: String,
    pub maturity_mode: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Level {
    R0,
    R1,
    R2,
    R3,
    R4,
    R5,
    R6,
    R7,
    R8,
    R9,
    R10,
}

impl Level {
    pub fn number(self) -> u8 {
        match self {
            Self::R0 => 0,
            Self::R1 => 1,
            Self::R2 => 2,
            Self::R3 => 3,
            Self::R4 => 4,
            Self::R5 => 5,
            Self::R6 => 6,
            Self::R7 => 7,
            Self::R8 => 8,
            Self::R9 => 9,
            Self::R10 => 10,
        }
    }
}

impl std::fmt::Display for Level {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "R{}", self.number())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromotionCandidate {
    pub from: Level,
    pub to: Level,
    pub status: ClaimStatus,
    #[serde(default)]
    pub blocked_by: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimStatus {
    Pass,
    Warn,
    Blocked,
    NotApplicable,
}

impl std::fmt::Display for ClaimStatus {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            Self::Pass => "pass",
            Self::Warn => "warn",
            Self::Blocked => "blocked",
            Self::NotApplicable => "not_applicable",
        };
        formatter.write_str(label)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Axes {
    pub spec: Axis,
    pub contracts: Axis,
    pub core: Axis,
    pub security: Axis,
    pub ux: Axis,
    pub persistence: Axis,
    pub orchestration: Axis,
    pub inspection: Axis,
    pub release: Axis,
    pub operations: Axis,
    pub commercial_readiness: Axis,
    pub learning_yield: Axis,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Axis {
    pub level: Level,
    pub status: ClaimStatus,
    #[serde(default)]
    pub evidence: Vec<String>,
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformReadiness {
    pub cli: PlatformState,
    pub api: PlatformState,
    pub web: PlatformState,
    pub desktop: PlatformState,
    pub mobile: PlatformState,
    pub self_hosted: PlatformState,
    pub cloud_eu: PlatformState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlatformState {
    None,
    Planned,
    Proof,
    Usable,
    Trusted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceRef {
    pub kind: String,
    #[serde(rename = "ref")]
    pub reference: String,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningYield {
    pub kind: String,
    pub description: String,
    #[serde(default)]
    pub owner: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractionPressure {
    pub capability: String,
    pub seen_in: Vec<String>,
    pub suggested_owner: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaturityFinding {
    pub severity: FindingSeverity,
    pub code: String,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingSeverity {
    Error,
    Warning,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaturityReport {
    pub project: String,
    pub current_level: Level,
    pub target_level: Level,
    pub next_level: Option<Level>,
    pub status: ClaimStatus,
    pub blocked_by: Vec<String>,
    pub findings: Vec<MaturityFinding>,
    pub evidence_count: usize,
    pub learning_yield_count: usize,
}

impl MaturityReport {
    pub fn is_valid(&self) -> bool {
        !self
            .findings
            .iter()
            .any(|finding| finding.severity == FindingSeverity::Error)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaturityWorkspaceReport {
    pub reports: Vec<MaturityReport>,
}

impl MaturityWorkspaceReport {
    pub fn is_valid(&self) -> bool {
        self.reports.iter().all(MaturityReport::is_valid)
    }
}

pub fn validate_file(path: &Path) -> Result<MaturityReport> {
    let claim = load_claim(path)?;
    Ok(validate_claim(&claim))
}

pub fn report_dir(path: &Path) -> Result<MaturityWorkspaceReport> {
    let mut files = maturity_files(path)?;
    files.sort();
    let mut reports = Vec::new();
    for file in files {
        reports.push(validate_file(&file)?);
    }
    Ok(MaturityWorkspaceReport { reports })
}

fn maturity_files(path: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let read_dir = fs::read_dir(path).map_err(|source| Error::Io {
        path: path.display().to_string(),
        source,
    })?;
    for entry in read_dir {
        let entry = entry.map_err(|source| Error::Io {
            path: path.display().to_string(),
            source,
        })?;
        let file_type = entry.file_type().map_err(|source| Error::Io {
            path: entry.path().display().to_string(),
            source,
        })?;
        if file_type.is_file() && entry.path().extension().is_some_and(|ext| ext == "json") {
            files.push(entry.path());
        }
    }
    if files.is_empty() {
        return Err(Error::InvalidMaturity {
            message: format!("no maturity JSON files found in `{}`", path.display()),
        });
    }
    Ok(files)
}

fn load_claim(path: &Path) -> Result<MaturityClaim> {
    let raw = fs::read_to_string(path).map_err(|source| Error::Io {
        path: path.display().to_string(),
        source,
    })?;
    serde_json::from_str(&raw).map_err(|source| Error::InvalidMaturity {
        message: format!("{}: {source}", path.display()),
    })
}

pub fn validate_claim(claim: &MaturityClaim) -> MaturityReport {
    let mut findings = Vec::new();
    let mut blocked_by = Vec::new();

    if claim.format != "rumble.delivery_maturity.v0.1" {
        error(
            &mut findings,
            "invalid_format",
            format!("unsupported format `{}`", claim.format),
        );
    }
    if claim.project.layer != "Rumble" {
        error(
            &mut findings,
            "invalid_layer",
            format!(
                "maturity claims are Rumble-only, got `{}`",
                claim.project.layer
            ),
        );
    }
    if claim.target_level < claim.current_level {
        error(
            &mut findings,
            "target_below_current",
            format!(
                "target {} is below current {}",
                claim.target_level, claim.current_level
            ),
        );
    }
    if let Some(next) = claim.next_level
        && next < claim.current_level
    {
        error(
            &mut findings,
            "next_below_current",
            format!("next {next} is below current {}", claim.current_level),
        );
    }

    if claim.current_level >= Level::R7
        && (claim.axes.core.level < Level::R2
            || matches!(
                claim.axes.core.status,
                ClaimStatus::Blocked | ClaimStatus::NotApplicable
            ))
    {
        error(
            &mut findings,
            "mobile_without_portable_core",
            "R7 mobile maturity requires at least R2 portable core evidence".to_string(),
        );
    }

    if claim.current_level >= Level::R10 {
        if claim.axes.security.status != ClaimStatus::Pass {
            error(
                &mut findings,
                "commercializable_security_not_passing",
                "R10 commercializable requires passing security axis".to_string(),
            );
        }
        if claim.axes.release.status != ClaimStatus::Pass {
            error(
                &mut findings,
                "commercializable_release_not_passing",
                "R10 commercializable requires passing release axis".to_string(),
            );
        }
        if !claim.open_questions.is_empty() {
            error(
                &mut findings,
                "commercializable_open_questions",
                "R10 commercializable cannot hide open questions".to_string(),
            );
        }
    }

    if let Some(promotion) = &claim.promotion_candidate {
        if promotion.status == ClaimStatus::Blocked && promotion.blocked_by.is_empty() {
            error(
                &mut findings,
                "blocked_promotion_without_blockers",
                "blocked promotion requires blocked_by entries".to_string(),
            );
        }
        if promotion.status == ClaimStatus::Pass && !promotion.blocked_by.is_empty() {
            error(
                &mut findings,
                "passing_promotion_with_blockers",
                "passing promotion must not carry blocked_by entries".to_string(),
            );
        }
        blocked_by.extend(promotion.blocked_by.clone());
    }

    if claim.evidence.is_empty() {
        error(
            &mut findings,
            "missing_evidence",
            "maturity claim requires at least one evidence reference".to_string(),
        );
    }
    if claim.learning_yield.is_empty() {
        error(
            &mut findings,
            "missing_learning_yield",
            "Rumble dojo maturity claim requires learning_yield".to_string(),
        );
    }

    warn_on_blocked_axes(claim, &mut findings);

    let status = if findings
        .iter()
        .any(|finding| finding.severity == FindingSeverity::Error)
        || claim
            .promotion_candidate
            .as_ref()
            .is_some_and(|promotion| promotion.status == ClaimStatus::Blocked)
    {
        ClaimStatus::Blocked
    } else if findings
        .iter()
        .any(|finding| finding.severity == FindingSeverity::Warning)
    {
        ClaimStatus::Warn
    } else {
        ClaimStatus::Pass
    };

    MaturityReport {
        project: claim.project.name.clone(),
        current_level: claim.current_level,
        target_level: claim.target_level,
        next_level: claim.next_level,
        status,
        blocked_by,
        findings,
        evidence_count: claim.evidence.len(),
        learning_yield_count: claim.learning_yield.len(),
    }
}

fn warn_on_blocked_axes(claim: &MaturityClaim, findings: &mut Vec<MaturityFinding>) {
    for (name, axis) in [
        ("spec", &claim.axes.spec),
        ("contracts", &claim.axes.contracts),
        ("core", &claim.axes.core),
        ("security", &claim.axes.security),
        ("ux", &claim.axes.ux),
        ("persistence", &claim.axes.persistence),
        ("orchestration", &claim.axes.orchestration),
        ("inspection", &claim.axes.inspection),
        ("release", &claim.axes.release),
        ("operations", &claim.axes.operations),
        ("commercial_readiness", &claim.axes.commercial_readiness),
    ] {
        if axis.status == ClaimStatus::Blocked && axis.level >= claim.current_level {
            warning(
                findings,
                "blocked_axis_at_claimed_level",
                format!(
                    "axis `{name}` is blocked at {} while current claim is {}",
                    axis.level, claim.current_level
                ),
            );
        }
    }
}

fn error(findings: &mut Vec<MaturityFinding>, code: &str, detail: String) {
    findings.push(MaturityFinding {
        severity: FindingSeverity::Error,
        code: code.to_string(),
        detail,
    });
}

fn warning(findings: &mut Vec<MaturityFinding>, code: &str, detail: String) {
    findings.push(MaturityFinding {
        severity: FindingSeverity::Warning,
        code: code.to_string(),
        detail,
    });
}
