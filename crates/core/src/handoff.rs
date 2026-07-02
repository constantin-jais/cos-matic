//! Planning-only Rumble → Bolt handoff validation.
//!
//! This module is deliberately small and deterministic. It validates the first
//! `ImplementationHandoff` contract before any product UI starts depending on
//! Bolt execution. MVP scope: validate/refuse/produce a dry-run planning report;
//! never execute implementation work.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use crate::error::{Error, Result};

const SUPPORTED_FORMAT: &str = "canvas.bolt_handoff.v0.1";
const SUPPORTED_KIND: &str = "planning_request";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingSeverity {
    Error,
    Warning,
}

impl FindingSeverity {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HandoffFinding {
    pub severity: FindingSeverity,
    pub code: &'static str,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HandoffReport {
    pub format: Option<String>,
    pub kind: Option<String>,
    pub handoff_id: Option<String>,
    pub package_id: Option<String>,
    pub package_hash: Option<String>,
    pub planning_goal: Option<String>,
    pub requested_outputs: Vec<String>,
    pub findings: Vec<HandoffFinding>,
    #[serde(skip)]
    pub traceability_links_count: usize,
}

impl HandoffReport {
    pub fn is_valid(&self) -> bool {
        !self
            .findings
            .iter()
            .any(|finding| finding.severity == FindingSeverity::Error)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DryRunPlan {
    pub report: HandoffReport,
    pub gates: Vec<PlanGate>,
    pub tasks: Vec<PlanTask>,
    pub next_actions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlanGate {
    pub code: String,
    pub status: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlanTask {
    pub title: String,
    pub source_trace: Vec<String>,
    pub acceptance: Vec<String>,
}

pub fn validate_file(path: &Path) -> Result<HandoffReport> {
    let source = fs::read_to_string(path).map_err(|source| Error::Io {
        path: path.display().to_string(),
        source,
    })?;
    validate_str(&source)
}

pub fn validate_str(source: &str) -> Result<HandoffReport> {
    let payload: Value = serde_json::from_str(source).map_err(|err| Error::InvalidHandoff {
        message: format!("invalid JSON: {err}"),
    })?;
    Ok(validate_payload(&payload))
}

pub fn dry_run_plan_file(path: &Path) -> Result<DryRunPlan> {
    dry_run_plan_file_with_evidence_sources(path, &[], &[], &[])
}

pub fn dry_run_plan_file_with_evidence_reports(
    path: &Path,
    evidence_report_paths: &[PathBuf],
) -> Result<DryRunPlan> {
    dry_run_plan_file_with_evidence_sources(path, evidence_report_paths, &[], &[])
}

pub fn dry_run_plan_file_with_evidence_reports_and_manifests(
    path: &Path,
    evidence_report_paths: &[PathBuf],
    evidence_manifest_paths: &[PathBuf],
) -> Result<DryRunPlan> {
    dry_run_plan_file_with_evidence_sources(
        path,
        evidence_report_paths,
        evidence_manifest_paths,
        &[],
    )
}

pub fn dry_run_plan_file_with_evidence_sources(
    path: &Path,
    evidence_report_paths: &[PathBuf],
    evidence_manifest_paths: &[PathBuf],
    human_approval_paths: &[PathBuf],
) -> Result<DryRunPlan> {
    dry_run_plan_file_with_evidence_sources_and_approval_keys(
        path,
        evidence_report_paths,
        evidence_manifest_paths,
        human_approval_paths,
        None,
    )
}

pub fn dry_run_plan_file_with_evidence_sources_and_approval_keys(
    path: &Path,
    evidence_report_paths: &[PathBuf],
    evidence_manifest_paths: &[PathBuf],
    human_approval_paths: &[PathBuf],
    approval_key_registry_path: Option<&Path>,
) -> Result<DryRunPlan> {
    let source = fs::read_to_string(path).map_err(|source| Error::Io {
        path: path.display().to_string(),
        source,
    })?;
    let mut payload: Value =
        serde_json::from_str(&source).map_err(|err| Error::InvalidHandoff {
            message: format!("invalid JSON: {err}"),
        })?;
    let approval_key_registry = ApprovalKeyRegistry::load_optional(approval_key_registry_path)?;
    inject_evidence_sources(
        &mut payload,
        evidence_report_paths,
        evidence_manifest_paths,
        human_approval_paths,
        &approval_key_registry,
    )?;
    let report = validate_payload(&payload);
    Ok(dry_run_plan_from_report_and_payload(&report, &payload))
}

fn dry_run_plan_from_report_and_payload(report: &HandoffReport, payload: &Value) -> DryRunPlan {
    let mut gates = derive_gates_from_evidence(report, payload);
    let mut tasks = Vec::new();
    let mut next_actions = Vec::new();

    if report.is_valid() {
        for output in &report.requested_outputs {
            tasks.push(task_for_output(output));
        }
        next_actions.push("review dry-run plan with a human owner".to_string());
        next_actions.push("run Wrench inspections before any execution approval".to_string());
    } else {
        gates.push(gate(
            "refuse_until_validation_clean",
            "blocked",
            "handoff has blocking validation errors",
        ));
        next_actions.push("fix validation findings and resubmit handoff".to_string());
    }

    DryRunPlan {
        report: report.clone(),
        gates,
        tasks,
        next_actions,
    }
}

fn inject_evidence_sources(
    payload: &mut Value,
    evidence_report_paths: &[PathBuf],
    evidence_manifest_paths: &[PathBuf],
    human_approval_paths: &[PathBuf],
    approval_key_registry: &ApprovalKeyRegistry,
) -> Result<()> {
    let mut projected_evidence_refs = Vec::new();
    let mut projected_artifact_refs = Vec::new();

    for evidence_report_path in evidence_report_paths {
        let source = fs::read_to_string(evidence_report_path).map_err(|source| Error::Io {
            path: evidence_report_path.display().to_string(),
            source,
        })?;
        let evidence_report: Value =
            serde_json::from_str(&source).map_err(|err| Error::InvalidHandoff {
                message: format!(
                    "invalid Wrench evidence report `{}`: {err}",
                    evidence_report_path.display()
                ),
            })?;
        projected_evidence_refs.push(project_wrench_evidence_report(
            &evidence_report,
            evidence_report_path,
        )?);
    }

    for evidence_manifest_path in evidence_manifest_paths {
        let source = fs::read_to_string(evidence_manifest_path).map_err(|source| Error::Io {
            path: evidence_manifest_path.display().to_string(),
            source,
        })?;
        let evidence_manifest: Value =
            serde_json::from_str(&source).map_err(|err| Error::InvalidHandoff {
                message: format!(
                    "invalid Gear evidence manifest `{}`: {err}",
                    evidence_manifest_path.display()
                ),
            })?;
        let (evidence_ref, artifact_ref) =
            project_gear_wrench_evidence_manifest(&evidence_manifest, evidence_manifest_path)?;
        projected_evidence_refs.push(evidence_ref);
        projected_artifact_refs.push(artifact_ref);
    }

    for human_approval_path in human_approval_paths {
        let source = fs::read(human_approval_path).map_err(|source| Error::Io {
            path: human_approval_path.display().to_string(),
            source,
        })?;
        let human_approval: Value =
            serde_json::from_slice(&source).map_err(|err| Error::InvalidHandoff {
                message: format!(
                    "invalid human approval `{}`: {err}",
                    human_approval_path.display()
                ),
            })?;
        projected_evidence_refs.push(project_human_approval(
            &human_approval,
            &source,
            human_approval_path,
            payload,
            approval_key_registry,
        )?);
    }

    append_projected_refs(
        payload,
        "evidence_refs",
        projected_evidence_refs,
        "--evidence-report/--evidence-manifest/--human-approval",
    )?;
    append_projected_refs(
        payload,
        "artifact_refs",
        projected_artifact_refs,
        "--evidence-manifest",
    )?;

    Ok(())
}

fn append_projected_refs(
    payload: &mut Value,
    field: &'static str,
    refs: Vec<Value>,
    source_flag: &'static str,
) -> Result<()> {
    if refs.is_empty() {
        return Ok(());
    }

    if let Value::Object(map) = payload {
        let target = map.entry(field).or_insert_with(|| Value::Array(Vec::new()));
        match target {
            Value::Array(values) => values.extend(refs),
            _ => {
                return Err(Error::InvalidHandoff {
                    message: format!("handoff {field} must be an array when {source_flag} is used"),
                });
            }
        }
    }

    Ok(())
}

fn project_wrench_evidence_report(report: &Value, path: &Path) -> Result<Value> {
    if string_at(report, &["format"]).as_deref() != Some("wrench.evidence_report.v0.1") {
        return Err(Error::InvalidHandoff {
            message: format!(
                "evidence report `{}` must use format wrench.evidence_report.v0.1",
                path.display()
            ),
        });
    }

    let report_id = required_string(report, &["report_id"], path)?;
    let status = required_string(report, &["status"], path)?;
    let hash = required_string(report, &["source_report", "hash"], path)?;
    if !is_valid_sha256(&hash) {
        return Err(Error::InvalidHandoff {
            message: format!(
                "evidence report `{}` source_report.hash must be sha256:<64 hex chars>",
                path.display()
            ),
        });
    }
    let producer = string_at(report, &["producer", "name"])
        .filter(|producer| !producer.trim().is_empty())
        .unwrap_or_else(|| "wrench-inspect".to_string());
    let summary = source_report_summary(report, &report_id, &status);

    Ok(json!({
        "kind": "wrench.evidence_report.v0.1",
        "producer": producer,
        "ref": report_id,
        "hash": hash,
        "status": status,
        "summary": summary,
        "metadata": {
            "source": "--evidence-report",
            "source_format": "wrench.evidence_report.v0.1"
        }
    }))
}

fn project_gear_wrench_evidence_manifest(manifest: &Value, path: &Path) -> Result<(Value, Value)> {
    let manifest_id = required_manifest_string(manifest, &["manifest_id"], path)?;
    let artifact_id = required_manifest_string(manifest, &["artifact", "artifact_id"], path)?;
    let artifact_type = required_manifest_string(manifest, &["artifact", "artifact_type"], path)?;
    if artifact_type != "inspection_report" {
        return Err(Error::InvalidHandoff {
            message: format!(
                "Gear evidence manifest `{}` artifact.artifact_type must be inspection_report",
                path.display()
            ),
        });
    }

    let producer = required_manifest_string(manifest, &["artifact", "producer"], path)?;
    if producer != "wrench-inspect" {
        return Err(Error::InvalidHandoff {
            message: format!(
                "Gear evidence manifest `{}` artifact.producer must be wrench-inspect",
                path.display()
            ),
        });
    }

    let manifest_ref = required_manifest_string(manifest, &["artifact", "manifest_ref"], path)?;
    if manifest_ref != manifest_id {
        return Err(Error::InvalidHandoff {
            message: format!(
                "Gear evidence manifest `{}` artifact.manifest_ref must match manifest_id",
                path.display()
            ),
        });
    }

    let artifact_hash = required_manifest_string(manifest, &["artifact", "hash"], path)?;
    if !is_valid_sha256(&artifact_hash) {
        return Err(Error::InvalidHandoff {
            message: format!(
                "Gear evidence manifest `{}` artifact.hash must be sha256:<64 hex chars>",
                path.display()
            ),
        });
    }

    let source_format =
        required_manifest_string(manifest, &["metadata", "values", "source_format"], path)?;
    if source_format != "wrench.evidence_report.v0.1" {
        return Err(Error::InvalidHandoff {
            message: format!(
                "Gear evidence manifest `{}` metadata.values.source_format must be wrench.evidence_report.v0.1",
                path.display()
            ),
        });
    }

    let status =
        required_manifest_string(manifest, &["metadata", "values", "evidence_status"], path)?;
    let state =
        string_at(manifest, &["artifact", "state"]).unwrap_or_else(|| "unknown".to_string());
    let source_report_hash = string_at(manifest, &["metadata", "values", "source_report_hash"])
        .unwrap_or_else(|| "unknown".to_string());
    let subject_ref = string_at(manifest, &["metadata", "values", "subject_ref"])
        .unwrap_or_else(|| "unknown".to_string());
    let summary = format!(
        "Gear ArtifactManifest {manifest_id}: producer={producer}, status={status}, artifact_state={state}"
    );

    let evidence_ref = json!({
        "kind": "wrench.evidence_report.v0.1",
        "producer": producer,
        "ref": artifact_id,
        "artifact_reference_id": artifact_id,
        "hash": artifact_hash,
        "status": status,
        "summary": summary,
        "metadata": {
            "source": "--evidence-manifest",
            "source_format": source_format,
            "manifest_id": manifest_id,
            "source_report_hash": source_report_hash,
            "subject_ref": subject_ref
        }
    });
    let artifact_ref = json!({
        "kind": "artifact_ref",
        "artifact_reference_id": artifact_id,
        "artifact_kind": "wrench_report",
        "artifact_type": artifact_type,
        "manifest_version": "gear.artifact_manifest.v0.1",
        "artifact_hash": artifact_hash,
        "manifest_ref": manifest_id,
        "producer": producer,
        "state": state,
        "metadata": {
            "source": "--evidence-manifest",
            "source_format": source_format
        }
    });

    Ok((evidence_ref, artifact_ref))
}

fn project_human_approval(
    approval: &Value,
    bytes: &[u8],
    path: &Path,
    handoff: &Value,
    approval_key_registry: &ApprovalKeyRegistry,
) -> Result<Value> {
    if string_at(approval, &["format"]).as_deref() != Some("bolt.human_approval.v0.1") {
        return Err(Error::InvalidHandoff {
            message: format!(
                "human approval `{}` must use format bolt.human_approval.v0.1",
                path.display()
            ),
        });
    }

    let approval_id = required_approval_string(approval, &["approval_id"], path)?;
    let subject_kind = required_approval_string(approval, &["subject", "kind"], path)?;
    let subject_ref = required_approval_string(approval, &["subject", "ref"], path)?;
    let subject_hash = required_approval_string(approval, &["subject", "hash"], path)?;
    if !is_valid_sha256(&subject_hash) {
        return Err(Error::InvalidHandoff {
            message: format!(
                "human approval `{}` subject.hash must be sha256:<64 hex chars>",
                path.display()
            ),
        });
    }

    let decision = required_approval_string(approval, &["decision"], path)?;
    let approved_by = required_approval_string(approval, &["approved_by"], path)?;
    let approved_at = required_approval_string(approval, &["approved_at"], path)?;
    let expires_at = required_approval_string(approval, &["expires_at"], path)?;
    validate_canonical_utc_timestamp(&approved_at, "approved_at", "human approval", path)?;
    validate_canonical_utc_timestamp(&expires_at, "expires_at", "human approval", path)?;
    let signature_algorithm =
        required_approval_string(approval, &["signature", "algorithm"], path)?;
    let signer_ref = required_approval_string(approval, &["signature", "public_key_ref"], path)?;
    let signature_value = required_approval_string(approval, &["signature", "value"], path)?;
    let now = OffsetDateTime::now_utc();

    if signature_algorithm != "ed25519" {
        return Err(Error::InvalidHandoff {
            message: format!(
                "human approval `{}` signature.algorithm must be ed25519",
                path.display()
            ),
        });
    }

    let expected_ref = string_at(handoff, &["source", "handoff_id"])
        .unwrap_or_else(|| "<missing-handoff-id>".to_string());
    let expected_hash =
        approval_subject_hash(handoff).unwrap_or_else(|| "<missing-package-hash>".to_string());
    let mut failures = Vec::new();

    if subject_kind != "handoff_package" {
        failures.push(format!(
            "subject.kind must be handoff_package, got `{subject_kind}`"
        ));
    }
    if subject_ref != expected_ref {
        failures.push(format!(
            "subject.ref `{subject_ref}` does not match handoff `{expected_ref}`"
        ));
    }
    if subject_hash != expected_hash {
        failures.push("subject.hash does not match the handoff package hash".to_string());
    }
    if decision != "approved" {
        failures.push(format!("decision is `{decision}`"));
    }
    match parse_rfc3339_timestamp(&expires_at) {
        Some(expires_at_time) if expires_at_time <= now => {
            failures.push(format!("approval expired at `{expires_at}`"));
        }
        Some(_) => {}
        None => failures.push(format!(
            "approval expires_at `{expires_at}` must be RFC3339"
        )),
    }

    let signature_fields = HumanApprovalSignatureFields {
        approval_id: &approval_id,
        subject_kind: &subject_kind,
        subject_ref: &subject_ref,
        subject_hash: &subject_hash,
        decision: &decision,
        approved_by: &approved_by,
        approved_at: &approved_at,
        expires_at: &expires_at,
    };
    let canonical_message = human_approval_signature_message(&signature_fields);
    let key_lookup = approval_key_registry.lookup(&signer_ref);
    let key_lookup_status = if key_lookup.is_some() {
        "found"
    } else {
        "unknown"
    };
    let key_state = key_lookup
        .map(|key| key.effective_state(now))
        .unwrap_or("unknown");
    let key_rotation_marker_count = key_lookup
        .map(ApprovalKeyRef::rotation_marker_count)
        .unwrap_or_default();
    let signature_verified = key_lookup.is_some_and(|key| {
        verify_human_approval_signature(
            &key.public_key,
            &signature_value,
            canonical_message.as_bytes(),
        )
    });

    if key_lookup_status != "found" {
        failures.push(format!(
            "approval key ref `{signer_ref}` is unknown in the registry"
        ));
    } else if key_state != "active" {
        failures.push(format!(
            "approval key ref `{signer_ref}` is not active ({key_state})"
        ));
    }
    if !signature_verified {
        failures.push("signature verification failed".to_string());
    }

    let status = if failures.is_empty() {
        "passed"
    } else {
        "failed"
    };
    let state = human_approval_state(&expires_at, key_lookup_status, key_state, now);
    let summary = if failures.is_empty() {
        format!("Human approval {approval_id}: approved for handoff package {subject_ref}")
    } else {
        format!(
            "Human approval {approval_id}: not accepted for planning checkpoint ({})",
            failures.join("; ")
        )
    };

    Ok(json!({
        "kind": "human_approval",
        "producer": "human-approval",
        "ref": approval_id,
        "hash": sha256_prefixed(bytes),
        "status": status,
        "state": state,
        "summary": summary,
        "metadata": {
            "source": "--human-approval",
            "source_format": "bolt.human_approval.v0.1",
            "subject_kind": subject_kind,
            "subject_ref": subject_ref,
            "subject_hash": subject_hash,
            "approved_at": approved_at,
            "expires_at": expires_at,
            "signer_ref": signer_ref,
            "signature_algorithm": signature_algorithm,
            "signature_verified": signature_verified.to_string(),
            "key_lookup_status": key_lookup_status,
            "key_state": key_state,
            "key_rotation_marker_count": key_rotation_marker_count.to_string()
        }
    }))
}

fn approval_subject_hash(handoff: &Value) -> Option<String> {
    string_at(handoff, &["idempotency", "payload_hash"])
        .or_else(|| string_at(handoff, &["package", "package_hash"]))
}

fn human_approval_state(
    expires_at: &str,
    key_lookup_status: &str,
    key_state: &str,
    now: OffsetDateTime,
) -> &'static str {
    let Some(expires_at_time) = parse_rfc3339_timestamp(expires_at) else {
        return "invalid";
    };

    if expires_at_time <= now || key_state == "expired" {
        "expired"
    } else if key_state == "revoked" {
        "revoked"
    } else if key_state == "invalid_time" {
        "invalid"
    } else if key_lookup_status != "found" {
        "unknown"
    } else {
        "active"
    }
}

#[derive(Debug, Clone, Default)]
struct ApprovalKeyRegistry {
    keys: Vec<ApprovalKeyRef>,
}

impl ApprovalKeyRegistry {
    fn load_optional(path: Option<&Path>) -> Result<Self> {
        let Some(path) = path else {
            return Ok(Self::default());
        };
        let source = fs::read_to_string(path).map_err(|source| Error::Io {
            path: path.display().to_string(),
            source,
        })?;
        let registry: Value =
            serde_json::from_str(&source).map_err(|err| Error::InvalidHandoff {
                message: format!("invalid approval key registry `{}`: {err}", path.display()),
            })?;
        Self::from_value(&registry, path)
    }

    fn from_value(registry: &Value, path: &Path) -> Result<Self> {
        if string_at(registry, &["format"]).as_deref() != Some("bolt.approval_key_registry.v0.1") {
            return Err(Error::InvalidHandoff {
                message: format!(
                    "approval key registry `{}` must use format bolt.approval_key_registry.v0.1",
                    path.display()
                ),
            });
        }

        let keys = value_at(registry, &["keys"])
            .and_then(Value::as_array)
            .ok_or_else(|| Error::InvalidHandoff {
                message: format!(
                    "approval key registry `{}` must contain keys[]",
                    path.display()
                ),
            })?;
        if keys.is_empty() {
            return Err(Error::InvalidHandoff {
                message: format!(
                    "approval key registry `{}` must contain at least one key",
                    path.display()
                ),
            });
        }

        let mut seen_refs = HashSet::new();
        let mut parsed_keys = Vec::with_capacity(keys.len());

        for key in keys {
            let public_key_ref = required_registry_string(key, &["public_key_ref"], path)?;
            if !seen_refs.insert(public_key_ref.clone()) {
                return Err(Error::InvalidHandoff {
                    message: format!(
                        "approval key registry `{}` contains duplicate public_key_ref `{public_key_ref}`",
                        path.display()
                    ),
                });
            }

            let algorithm = required_registry_string(key, &["algorithm"], path)?;
            if algorithm != "ed25519" {
                return Err(Error::InvalidHandoff {
                    message: format!(
                        "approval key registry `{}` key `{public_key_ref}` algorithm must be ed25519",
                        path.display()
                    ),
                });
            }

            let state = required_registry_string(key, &["state"], path)?;
            if !matches!(state.as_str(), "active" | "revoked") {
                return Err(Error::InvalidHandoff {
                    message: format!(
                        "approval key registry `{}` key `{public_key_ref}` state must be active or revoked",
                        path.display()
                    ),
                });
            }

            let public_key = required_registry_string(key, &["public_key"], path)?;
            if decode_prefixed_hex(&public_key, "ed25519:", 32).is_none() {
                return Err(Error::InvalidHandoff {
                    message: format!(
                        "approval key registry `{}` key `{public_key_ref}` public_key must be ed25519:<32-byte hex>",
                        path.display()
                    ),
                });
            }
            let not_before = required_registry_string(key, &["not_before"], path)?;
            let expires_at = required_registry_string(key, &["expires_at"], path)?;
            let revoked_at = string_at(key, &["revoked_at"]);
            validate_canonical_utc_timestamp(
                &not_before,
                "not_before",
                "approval key registry",
                path,
            )?;
            validate_canonical_utc_timestamp(
                &expires_at,
                "expires_at",
                "approval key registry",
                path,
            )?;
            if let Some(revoked_at) = revoked_at.as_deref() {
                validate_canonical_utc_timestamp(
                    revoked_at,
                    "revoked_at",
                    "approval key registry",
                    path,
                )?;
            }

            parsed_keys.push(ApprovalKeyRef {
                public_key_ref,
                public_key,
                state,
                not_before: Some(not_before),
                expires_at,
                rotated_from: string_at(key, &["rotated_from"]),
                rotated_to: string_at(key, &["rotated_to"]),
                revoked_at,
            });
        }

        Ok(Self { keys: parsed_keys })
    }

    fn lookup(&self, public_key_ref: &str) -> Option<&ApprovalKeyRef> {
        self.keys
            .iter()
            .find(|key| key.public_key_ref == public_key_ref)
    }
}

#[derive(Debug, Clone)]
struct ApprovalKeyRef {
    public_key_ref: String,
    public_key: String,
    state: String,
    not_before: Option<String>,
    expires_at: String,
    rotated_from: Option<String>,
    rotated_to: Option<String>,
    revoked_at: Option<String>,
}

impl ApprovalKeyRef {
    fn effective_state(&self, now: OffsetDateTime) -> &'static str {
        if self.state == "revoked" || self.revoked_at.is_some() {
            return "revoked";
        }

        let Some(expires_at) = parse_rfc3339_timestamp(&self.expires_at) else {
            return "invalid_time";
        };
        if expires_at <= now {
            return "expired";
        }

        if let Some(not_before) = &self.not_before {
            let Some(not_before) = parse_rfc3339_timestamp(not_before) else {
                return "invalid_time";
            };
            if not_before > now {
                return "not_yet_valid";
            }
        }

        "active"
    }

    fn rotation_marker_count(&self) -> usize {
        self.rotated_from.iter().count() + self.rotated_to.iter().count()
    }
}

struct HumanApprovalSignatureFields<'a> {
    approval_id: &'a str,
    subject_kind: &'a str,
    subject_ref: &'a str,
    subject_hash: &'a str,
    decision: &'a str,
    approved_by: &'a str,
    approved_at: &'a str,
    expires_at: &'a str,
}

fn human_approval_signature_message(fields: &HumanApprovalSignatureFields<'_>) -> String {
    format!(
        "bolt.human_approval.v0.1\napproval_id:{}\nsubject.kind:{}\nsubject.ref:{}\nsubject.hash:{}\ndecision:{}\napproved_by:{}\napproved_at:{}\nexpires_at:{}",
        fields.approval_id,
        fields.subject_kind,
        fields.subject_ref,
        fields.subject_hash,
        fields.decision,
        fields.approved_by,
        fields.approved_at,
        fields.expires_at,
    )
}

fn verify_human_approval_signature(
    public_key: &str,
    signature_value: &str,
    message: &[u8],
) -> bool {
    let Some(public_key_bytes) = decode_prefixed_hex(public_key, "ed25519:", 32) else {
        return false;
    };
    let Some(signature_bytes) = decode_prefixed_hex(signature_value, "ed25519:", 64) else {
        return false;
    };

    let Ok(public_key_array) = <[u8; 32]>::try_from(public_key_bytes.as_slice()) else {
        return false;
    };
    let Ok(signature_array) = <[u8; 64]>::try_from(signature_bytes.as_slice()) else {
        return false;
    };
    let Ok(verifying_key) = VerifyingKey::from_bytes(&public_key_array) else {
        return false;
    };
    let signature = Signature::from_bytes(&signature_array);
    verifying_key.verify(message, &signature).is_ok()
}

fn decode_prefixed_hex(value: &str, prefix: &str, expected_bytes: usize) -> Option<Vec<u8>> {
    let hex = value.strip_prefix(prefix)?;
    if hex.len() != expected_bytes * 2 {
        return None;
    }

    let mut bytes = Vec::with_capacity(expected_bytes);
    for chunk in hex.as_bytes().chunks_exact(2) {
        let high = hex_nibble(chunk[0])?;
        let low = hex_nibble(chunk[1])?;
        bytes.push((high << 4) | low);
    }
    Some(bytes)
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn required_approval_string(approval: &Value, path: &[&str], source_path: &Path) -> Result<String> {
    string_at(approval, path)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| Error::InvalidHandoff {
            message: format!(
                "human approval `{}` is missing required string `{}`",
                source_path.display(),
                path.join(".")
            ),
        })
}

fn required_registry_string(registry: &Value, path: &[&str], source_path: &Path) -> Result<String> {
    string_at(registry, path)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| Error::InvalidHandoff {
            message: format!(
                "approval key registry `{}` is missing required string `{}`",
                source_path.display(),
                path.join(".")
            ),
        })
}

fn validate_canonical_utc_timestamp(
    value: &str,
    field: &str,
    source_kind: &str,
    source_path: &Path,
) -> Result<()> {
    if is_canonical_utc_second_timestamp(value) {
        return Ok(());
    }

    Err(Error::InvalidHandoff {
        message: format!(
            "{source_kind} `{}` field `{field}` must be UTC RFC3339 seconds like 2026-07-02T00:00:00Z",
            source_path.display()
        ),
    })
}

fn is_canonical_utc_second_timestamp(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 20
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes[10] == b'T'
        && bytes[13] == b':'
        && bytes[16] == b':'
        && bytes[19] == b'Z'
        && bytes.iter().enumerate().all(|(index, byte)| {
            matches!(index, 4 | 7 | 10 | 13 | 16 | 19) || byte.is_ascii_digit()
        })
}

fn required_manifest_string(manifest: &Value, path: &[&str], source_path: &Path) -> Result<String> {
    string_at(manifest, path)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| Error::InvalidHandoff {
            message: format!(
                "Gear evidence manifest `{}` is missing required string `{}`",
                source_path.display(),
                path.join(".")
            ),
        })
}

fn required_string(report: &Value, path: &[&str], source_path: &Path) -> Result<String> {
    string_at(report, path)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| Error::InvalidHandoff {
            message: format!(
                "evidence report `{}` is missing required string `{}`",
                source_path.display(),
                path.join(".")
            ),
        })
}

fn source_report_summary(report: &Value, report_id: &str, status: &str) -> String {
    let errors = value_at(report, &["summary", "errors"])
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let warnings = value_at(report, &["summary", "warnings"])
        .and_then(Value::as_u64)
        .unwrap_or(0);
    format!(
        "Wrench EvidenceReport {report_id}: status={status}, errors={errors}, warnings={warnings}"
    )
}

fn derive_gates_from_evidence(report: &HandoffReport, payload: &Value) -> Vec<PlanGate> {
    let mut gates = Vec::new();

    // package_approved: check if package data is present and valid
    let package_id = string_at(payload, &["package", "package_id"]);
    let items = array_at(payload, &["package", "items"]);
    let package_approved_status = if package_id.is_some() && !items.is_empty() {
        "pass"
    } else {
        "blocked"
    };
    gates.push(gate(
        "package_approved",
        package_approved_status,
        "package metadata and items are present",
    ));

    // planning_only_enforced: verify execution policy
    let planning_only = bool_at(payload, &["execution_policy", "planning_only"]);
    let allow_execution = bool_at(payload, &["execution_policy", "allow_execution"]);
    let requires_approval = bool_at(
        payload,
        &["execution_policy", "requires_human_approval_for_execution"],
    );
    let planning_enforced = planning_only == Some(true)
        && allow_execution == Some(false)
        && requires_approval == Some(true);
    gates.push(gate(
        "planning_only_enforced",
        if planning_enforced { "pass" } else { "blocked" },
        "execution is explicitly forbidden",
    ));

    // traceability_present: derived from count in report
    let traceability_status = if report.traceability_links_count > 0 {
        "pass"
    } else {
        "blocked"
    };
    gates.push(gate(
        "traceability_present",
        traceability_status,
        "traceability links are present",
    ));

    // blocking_questions_waived_or_absent: check for unwaived blocking questions
    let has_accepted_waiver = has_accepted_waiver(payload);
    let has_unwaived_blocker = array_at(payload, &["open_questions"]).iter().any(|q| {
        string_field(q, "impact").as_deref() == Some("blocking")
            && string_field(q, "status").as_deref() == Some("open")
            && !has_accepted_waiver
    });
    gates.push(gate(
        "blocking_questions_waived_or_absent",
        if !has_unwaived_blocker {
            "pass"
        } else {
            "blocked"
        },
        "no unwaived blocking question detected",
    ));

    // high_risks_waived_or_absent: check for unwaived high/critical risks
    let has_unwaived_high_risk = array_at(payload, &["risks"]).iter().any(|r| {
        let severity = string_field(r, "severity");
        let status = string_field(r, "status");
        let is_high = matches!(
            severity.as_deref(),
            Some("high") | Some("critical") | Some("blocking")
        );
        let is_open = !matches!(
            status.as_deref(),
            Some("mitigated") | Some("accepted") | Some("waived")
        );
        is_high && is_open && !has_accepted_waiver
    });
    gates.push(gate(
        "high_risks_waived_or_absent",
        if !has_unwaived_high_risk {
            "pass"
        } else {
            "blocked"
        },
        "no unwaived high/critical risk detected",
    ));

    gates.push(derive_wrench_report_gate(payload));

    // shared_capability_review_requested: skeleton gate (static, not yet wired)
    gates.push(gate_skeleton(
        "shared_capability_review_requested",
        "pass",
        "shared capability extraction review can be produced",
    ));

    // Additional gates for future integrations.
    gates.push(derive_human_approval_checkpoint_gate(payload));
    gates.push(derive_artifact_supply_chain_gate(payload));
    gates.push(gate_skeleton(
        "sovereignty_and_license_audit",
        "pass",
        "external audit tool integration pending",
    ));

    gates
}

fn gate(code: &str, status: &str, detail: &str) -> PlanGate {
    PlanGate {
        code: code.to_string(),
        status: status.to_string(),
        detail: detail.to_string(),
    }
}

fn gate_skeleton(code: &str, status: &str, detail: &str) -> PlanGate {
    PlanGate {
        code: code.to_string(),
        status: status.to_string(),
        detail: format!("{} [skeleton (static)]", detail),
    }
}

fn task_for_output(output: &str) -> PlanTask {
    let title = match output {
        "implementation_plan" => "Draft implementation plan",
        "task_breakdown" => "Derive task breakdown",
        "risk_review" => "Review risks and waivers",
        "test_plan" => "Derive acceptance and contract test plan",
        "shared_capability_extraction_review" => "Review shared capability extraction",
        other => {
            return PlanTask {
                title: format!("Produce requested output `{other}`"),
                source_trace: Vec::new(),
                acceptance: vec![format!("Output `{other}` is present in the dry-run report")],
            };
        }
    };

    PlanTask {
        title: title.to_string(),
        source_trace: Vec::new(),
        acceptance: vec![format!("{title} is produced without execution")],
    }
}

fn validate_payload(payload: &Value) -> HandoffReport {
    let mut findings = Vec::new();

    let format = string_at(payload, &["format"]);
    if format.as_deref() != Some(SUPPORTED_FORMAT) {
        findings.push(error(
            "unsupported_format",
            format!(
                "expected format `{SUPPORTED_FORMAT}`, got `{}`",
                format.as_deref().unwrap_or("<missing>")
            ),
        ));
    }

    let kind = string_at(payload, &["kind"]);
    if kind.as_deref() != Some(SUPPORTED_KIND) {
        findings.push(error(
            "unsupported_kind",
            format!(
                "expected kind `{SUPPORTED_KIND}`, got `{}`",
                kind.as_deref().unwrap_or("<missing>")
            ),
        ));
    }

    let handoff_id = string_at(payload, &["source", "handoff_id"]);
    require_string(
        &mut findings,
        payload,
        &["source", "product"],
        "missing_source_product",
    );
    require_string(
        &mut findings,
        payload,
        &["source", "workspace_id"],
        "missing_workspace_id",
    );
    require_string(
        &mut findings,
        payload,
        &["source", "handoff_id"],
        "missing_handoff_id",
    );
    require_string(
        &mut findings,
        payload,
        &["source", "created_by"],
        "missing_created_by",
    );
    require_string(
        &mut findings,
        payload,
        &["source", "created_at"],
        "missing_created_at",
    );

    let package_id = string_at(payload, &["package", "package_id"]);
    let package_hash = string_at(payload, &["package", "package_hash"]);
    require_string(
        &mut findings,
        payload,
        &["package", "package_id"],
        "missing_package_id",
    );
    require_string(
        &mut findings,
        payload,
        &["package", "version"],
        "missing_package_version",
    );
    validate_package_hash(&mut findings, package_hash.as_deref());
    validate_artifact_integrity(payload, &mut findings);
    validate_artifact_refs(payload, &mut findings);
    validate_human_approval_refs(payload, &mut findings);
    validate_non_empty_array(
        &mut findings,
        payload,
        &["package", "items"],
        "empty_package_items",
    );

    let planning_goal = string_at(payload, &["planning_scope", "goal"]);
    require_string(
        &mut findings,
        payload,
        &["planning_scope", "mode"],
        "missing_planning_mode",
    );
    require_string(
        &mut findings,
        payload,
        &["planning_scope", "goal"],
        "missing_planning_goal",
    );

    validate_non_empty_array(
        &mut findings,
        payload,
        &["traceability_links"],
        "missing_traceability_links",
    );
    validate_waivers(payload, &mut findings);
    validate_blockers(payload, &mut findings);
    validate_risks(payload, &mut findings);
    validate_capability_candidates(payload, &mut findings);
    validate_sovereignty(payload, &mut findings);
    validate_handoff_hash_conflict(payload, &mut findings);
    validate_evidence_refs(payload, &mut findings);
    validate_execution_policy(payload, &mut findings);

    let requested_outputs = array_strings_at(payload, &["requested_outputs"]);
    if requested_outputs.is_empty() {
        findings.push(warning(
            "missing_requested_outputs",
            "no requested outputs declared; dry-run plan will be minimal".to_string(),
        ));
    }

    let traceability_links_count = array_at(payload, &["traceability_links"]).len();

    HandoffReport {
        format,
        kind,
        handoff_id,
        package_id,
        package_hash,
        planning_goal,
        requested_outputs,
        findings,
        traceability_links_count,
    }
}

fn validate_execution_policy(payload: &Value, findings: &mut Vec<HandoffFinding>) {
    let planning_only = bool_at(payload, &["execution_policy", "planning_only"]);
    let allow_execution = bool_at(payload, &["execution_policy", "allow_execution"]);
    let requires_human_approval = bool_at(
        payload,
        &["execution_policy", "requires_human_approval_for_execution"],
    );

    if planning_only != Some(true)
        || allow_execution != Some(false)
        || requires_human_approval != Some(true)
    {
        findings.push(error(
            "execution_policy_forbidden",
            "P0 handoffs must be planning-only, forbid execution, and require human approval for any future execution".to_string(),
        ));
    }
}

fn validate_package_hash(findings: &mut Vec<HandoffFinding>, hash: Option<&str>) {
    match hash {
        Some(value) if is_valid_sha256(value) => {}
        Some(value) => findings.push(error(
            "invalid_package_hash",
            format!("package_hash must be sha256:<64 hex chars>, got `{value}`"),
        )),
        None => findings.push(error(
            "missing_package_hash",
            "package.package_hash is required".to_string(),
        )),
    }
}

fn validate_artifact_integrity(payload: &Value, findings: &mut Vec<HandoffFinding>) {
    let artifact_reference_id = string_at(payload, &["package", "artifact_reference_id"]);
    let Some(artifact_reference_id) = artifact_reference_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return;
    };

    if string_at(payload, &["package", "artifact_hash"])
        .as_deref()
        .is_some_and(is_valid_sha256)
    {
        return;
    }

    let has_matching_artifact_ref = array_at(payload, &["artifact_refs"])
        .iter()
        .any(|artifact| {
            let ref_id = string_field(artifact, "artifact_reference_id")
                .or_else(|| string_field(artifact, "artifact_id"));
            let hash =
                string_field(artifact, "artifact_hash").or_else(|| string_field(artifact, "hash"));
            ref_id.as_deref() == Some(artifact_reference_id)
                && hash.as_deref().is_some_and(is_valid_sha256)
        });

    if !has_matching_artifact_ref {
        findings.push(error(
            "artifact_integrity_failed",
            "package artifact_reference_id requires package.artifact_hash or matching artifact_refs[].hash"
                .to_string(),
        ));
    }
}

fn validate_human_approval_refs(payload: &Value, findings: &mut Vec<HandoffFinding>) {
    for approval_ref in array_at(payload, &["evidence_refs"])
        .into_iter()
        .filter(|evidence_ref| is_human_approval_ref(evidence_ref))
    {
        if !string_field(approval_ref, "hash").is_some_and(|hash| is_valid_sha256(&hash)) {
            findings.push(error(
                "invalid_human_approval_hash",
                "human approval evidence hash must be sha256:<64 hex chars>".to_string(),
            ));
        }

        let key_lookup_status =
            value_at(approval_ref, &["metadata", "key_lookup_status"]).and_then(Value::as_str);
        if key_lookup_status != Some("found") {
            findings.push(error(
                "human_approval_key_unknown",
                "human approval public_key_ref must resolve in the approval key registry"
                    .to_string(),
            ));
        } else if value_at(approval_ref, &["metadata", "key_state"]).and_then(Value::as_str)
            != Some("active")
        {
            findings.push(error(
                "human_approval_key_not_active",
                "human approval public_key_ref must resolve to an active, non-revoked, non-expired key"
                    .to_string(),
            ));
        }

        if value_at(approval_ref, &["metadata", "signature_verified"]).and_then(Value::as_str)
            != Some("true")
        {
            findings.push(error(
                "human_approval_signature_invalid",
                "human approval signature must verify against the registered Ed25519 approval key"
                    .to_string(),
            ));
        }

        if string_field(approval_ref, "status").as_deref() != Some("passed") {
            findings.push(error(
                "human_approval_not_approved",
                format!(
                    "human approval status must be passed, got `{}`",
                    string_field(approval_ref, "status")
                        .as_deref()
                        .unwrap_or("<missing>")
                ),
            ));
        }

        if string_field(approval_ref, "state").as_deref() != Some("active") {
            findings.push(error(
                "human_approval_not_active",
                format!(
                    "human approval state must be active, got `{}`",
                    string_field(approval_ref, "state")
                        .as_deref()
                        .unwrap_or("<missing>")
                ),
            ));
        }
    }
}

fn validate_artifact_refs(payload: &Value, findings: &mut Vec<HandoffFinding>) {
    for artifact_ref in array_at(payload, &["artifact_refs"]) {
        if !string_field(artifact_ref, "artifact_hash").is_some_and(|hash| is_valid_sha256(&hash)) {
            findings.push(error(
                "invalid_artifact_hash",
                "artifact_refs[].artifact_hash must be sha256:<64 hex chars>".to_string(),
            ));
        }

        if !artifact_state_allows_planning(string_field(artifact_ref, "state").as_deref()) {
            findings.push(error(
                "artifact_not_active",
                format!(
                    "artifact ref state must be active when present, got `{}`",
                    string_field(artifact_ref, "state")
                        .as_deref()
                        .unwrap_or("<missing>")
                ),
            ));
        }
    }
}

fn parse_rfc3339_timestamp(value: &str) -> Option<OffsetDateTime> {
    OffsetDateTime::parse(value, &Rfc3339).ok()
}

fn sha256_prefixed(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    format!("sha256:{}", to_lower_hex(&digest))
}

fn to_lower_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    encoded
}

fn is_valid_sha256(value: &str) -> bool {
    value.starts_with("sha256:")
        && value.len() == "sha256:".len() + 64
        && value["sha256:".len()..]
            .chars()
            .all(|c| c.is_ascii_hexdigit())
}

fn validate_waivers(payload: &Value, findings: &mut Vec<HandoffFinding>) {
    for waiver in array_at(payload, &["active_waivers"]) {
        if let Some(expires_at) = string_field(waiver, "expires_at")
            && expires_at.as_str() < "2026-06-30T00:00:00Z"
        {
            findings.push(error(
                "expired_waiver",
                format!("waiver expired at `{expires_at}`"),
            ));
        }
    }
}

fn validate_blockers(payload: &Value, findings: &mut Vec<HandoffFinding>) {
    let has_accepted_waiver = has_accepted_waiver(payload);
    for question in array_at(payload, &["open_questions"]) {
        let impact = string_field(question, "impact");
        let status = string_field(question, "status");
        if impact.as_deref() == Some("blocking")
            && status.as_deref() == Some("open")
            && !has_accepted_waiver
        {
            findings.push(error(
                "blocking_question_without_waiver",
                "blocking open question requires an accepted active waiver".to_string(),
            ));
        }
    }
}

fn validate_risks(payload: &Value, findings: &mut Vec<HandoffFinding>) {
    let has_accepted_waiver = has_accepted_waiver(payload);
    for risk in array_at(payload, &["risks"]) {
        let severity = string_field(risk, "severity");
        let status = string_field(risk, "status");
        let high = matches!(
            severity.as_deref(),
            Some("high") | Some("critical") | Some("blocking")
        );
        let open = !matches!(
            status.as_deref(),
            Some("mitigated") | Some("accepted") | Some("waived")
        );
        if high && open && !has_accepted_waiver {
            findings.push(error(
                "high_risk_without_waiver",
                "high/critical open risk requires mitigation or an accepted active waiver"
                    .to_string(),
            ));
        }
    }
}

fn validate_capability_candidates(payload: &Value, findings: &mut Vec<HandoffFinding>) {
    for candidate in array_at(payload, &["capability_candidates"]) {
        let owner = string_field(candidate, "proposed_owner_layer");
        if owner.as_deref().unwrap_or_default().trim().is_empty() {
            findings.push(warning(
                "capability_owner_missing",
                "capability candidate has no proposed_owner_layer".to_string(),
            ));
        }
    }
}

fn validate_sovereignty(payload: &Value, findings: &mut Vec<HandoffFinding>) {
    let constraints = value_at(payload, &["constraints"]);
    let sovereignty = string_at(payload, &["constraints", "sovereignty"])
        .unwrap_or_default()
        .to_ascii_lowercase();
    let requires_external_saas = bool_at(payload, &["constraints", "requires_external_saas"]);
    let pii_in_logs_allowed = bool_at(payload, &["constraints", "pii_in_logs_allowed"]);
    let forbidden_dependencies = value_at(payload, &["constraints", "forbidden_dependencies"])
        .and_then(Value::as_array)
        .is_some_and(|values| !values.is_empty());

    let explicit_violation = ["violated", "mandatory us saas", "opaque storage", "non-eu"]
        .iter()
        .any(|needle| sovereignty.contains(needle));

    if constraints.is_some()
        && (explicit_violation
            || requires_external_saas == Some(true)
            || pii_in_logs_allowed == Some(true)
            || forbidden_dependencies)
    {
        findings.push(error(
            "sovereignty_policy_violation",
            "handoff violates sovereignty policy: no mandatory non-sovereign SaaS, opaque core-truth storage, forbidden dependencies, or PII in logs".to_string(),
        ));
    }
}

fn validate_handoff_hash_conflict(payload: &Value, findings: &mut Vec<HandoffFinding>) {
    let current = string_at(payload, &["idempotency", "payload_hash"])
        .or_else(|| string_at(payload, &["package", "package_hash"]));
    let prior = string_at(payload, &["idempotency", "prior_payload_hash"]);

    if let (Some(current), Some(prior)) = (current, prior)
        && current != prior
    {
        findings.push(error(
            "handoff_hash_conflict",
            "same handoff_id was previously associated with a different payload hash".to_string(),
        ));
    }
}

fn validate_evidence_refs(payload: &Value, findings: &mut Vec<HandoffFinding>) {
    for evidence_ref in array_at(payload, &["evidence_refs"])
        .into_iter()
        .filter(|evidence_ref| is_wrench_evidence_ref(evidence_ref))
    {
        validate_evidence_hash(evidence_ref, "hash", findings);
        validate_wrench_evidence_status(evidence_ref, "status", findings);
    }

    for report_ref in array_at(payload, &["wrench_report_refs"]) {
        validate_evidence_hash(report_ref, "report_hash", findings);
        validate_wrench_evidence_status(report_ref, "status", findings);
    }
}

fn validate_evidence_hash(
    evidence_ref: &Value,
    hash_field: &str,
    findings: &mut Vec<HandoffFinding>,
) {
    if !string_field(evidence_ref, hash_field).is_some_and(|hash| is_valid_sha256(&hash)) {
        findings.push(error(
            "invalid_evidence_hash",
            format!("Wrench evidence `{hash_field}` must be sha256:<64 hex chars>"),
        ));
    }
}

fn validate_wrench_evidence_status(
    evidence_ref: &Value,
    status_field: &str,
    findings: &mut Vec<HandoffFinding>,
) {
    let status = string_field(evidence_ref, status_field);
    if !wrench_status_allows_planning(status.as_deref()) {
        findings.push(error(
            "wrench_evidence_not_passing",
            format!(
                "Wrench evidence status must be passed or warning, got `{}`",
                status.as_deref().unwrap_or("<missing>")
            ),
        ));
    }
}

fn derive_human_approval_checkpoint_gate(payload: &Value) -> PlanGate {
    let allow_execution = bool_at(payload, &["execution_policy", "allow_execution"]);
    let requires_approval = bool_at(
        payload,
        &["execution_policy", "requires_human_approval_for_execution"],
    );

    if allow_execution == Some(true) {
        return gate(
            "human_approval_checkpoint",
            "blocked",
            "execution is requested; human approval workflow is not integrated in P0",
        );
    }

    let approval_refs: Vec<&Value> = array_at(payload, &["evidence_refs"])
        .into_iter()
        .filter(|evidence_ref| is_human_approval_ref(evidence_ref))
        .collect();
    if !approval_refs.is_empty() {
        let all_refs_ok = approval_refs.iter().all(|approval_ref| {
            string_field(approval_ref, "hash").is_some_and(|hash| is_valid_sha256(&hash))
                && string_field(approval_ref, "status").as_deref() == Some("passed")
                && string_field(approval_ref, "state").as_deref() == Some("active")
                && value_at(approval_ref, &["metadata", "key_lookup_status"])
                    .and_then(Value::as_str)
                    == Some("found")
                && value_at(approval_ref, &["metadata", "key_state"]).and_then(Value::as_str)
                    == Some("active")
                && value_at(approval_ref, &["metadata", "signature_verified"])
                    .and_then(Value::as_str)
                    == Some("true")
        });
        return gate(
            "human_approval_checkpoint",
            if all_refs_ok { "pass" } else { "blocked" },
            "human approval refs are present, registry-backed, active, approved, and hash-pinned",
        );
    }

    gate(
        "human_approval_checkpoint",
        if requires_approval == Some(true) {
            "pass"
        } else {
            "blocked"
        },
        "human approval is explicitly required before any future execution",
    )
}

fn derive_artifact_supply_chain_gate(payload: &Value) -> PlanGate {
    let artifact_refs = array_at(payload, &["artifact_refs"]);
    if artifact_refs.is_empty() {
        return gate_skeleton(
            "artifact_supply_chain_verified",
            "pass",
            "artifact registry integration pending",
        );
    }

    let all_refs_ok = artifact_refs.iter().all(|artifact_ref| {
        string_field(artifact_ref, "artifact_hash").is_some_and(|hash| is_valid_sha256(&hash))
            && artifact_state_allows_planning(string_field(artifact_ref, "state").as_deref())
    });

    gate(
        "artifact_supply_chain_verified",
        if all_refs_ok { "pass" } else { "blocked" },
        "Gear artifact refs are present, active when state is declared, and hash-pinned",
    )
}

fn derive_wrench_report_gate(payload: &Value) -> PlanGate {
    let evidence_refs = array_at(payload, &["evidence_refs"]);
    let wrench_evidence_refs: Vec<&Value> = evidence_refs
        .into_iter()
        .filter(|evidence_ref| is_wrench_evidence_ref(evidence_ref))
        .collect();
    let wrench_report_refs = array_at(payload, &["wrench_report_refs"]);

    if wrench_evidence_refs.is_empty() && wrench_report_refs.is_empty() {
        return gate_skeleton(
            "wrench_report_passed",
            "pass",
            "Wrench evidence ref integration pending for this handoff",
        );
    }

    let all_generic_refs_ok = wrench_evidence_refs.iter().all(|evidence_ref| {
        string_field(evidence_ref, "hash").is_some_and(|hash| is_valid_sha256(&hash))
            && wrench_status_allows_planning(string_field(evidence_ref, "status").as_deref())
    });
    let all_report_refs_ok = wrench_report_refs.iter().all(|report_ref| {
        string_field(report_ref, "report_hash").is_some_and(|hash| is_valid_sha256(&hash))
            && wrench_status_allows_planning(string_field(report_ref, "status").as_deref())
    });

    gate(
        "wrench_report_passed",
        if all_generic_refs_ok && all_report_refs_ok {
            "pass"
        } else {
            "blocked"
        },
        "Wrench evidence refs are present, hash-pinned, and not failed/quarantined",
    )
}

fn is_human_approval_ref(evidence_ref: &Value) -> bool {
    string_field(evidence_ref, "kind")
        .unwrap_or_default()
        .eq_ignore_ascii_case("human_approval")
}

fn is_wrench_evidence_ref(evidence_ref: &Value) -> bool {
    let kind = string_field(evidence_ref, "kind")
        .unwrap_or_default()
        .to_ascii_lowercase();
    let producer = string_field(evidence_ref, "producer")
        .unwrap_or_default()
        .to_ascii_lowercase();
    kind.contains("wrench") || producer == "wrench-inspect"
}

fn wrench_status_allows_planning(status: Option<&str>) -> bool {
    matches!(status, Some("passed") | Some("warning"))
}

fn artifact_state_allows_planning(state: Option<&str>) -> bool {
    matches!(state, None | Some("active"))
}

fn has_accepted_waiver(payload: &Value) -> bool {
    array_at(payload, &["active_waivers"]).iter().any(|waiver| {
        matches!(
            string_field(waiver, "status").as_deref(),
            Some("accepted") | Some("active")
        )
    })
}

fn require_string(
    findings: &mut Vec<HandoffFinding>,
    payload: &Value,
    path: &[&str],
    code: &'static str,
) {
    if string_at(payload, path).is_none() {
        findings.push(error(
            code,
            format!("missing required string `{}`", path.join(".")),
        ));
    }
}

fn validate_non_empty_array(
    findings: &mut Vec<HandoffFinding>,
    payload: &Value,
    path: &[&str],
    code: &'static str,
) {
    match value_at(payload, path).and_then(Value::as_array) {
        Some(values) if !values.is_empty() => {}
        _ => findings.push(error(
            code,
            format!("`{}` must be a non-empty array", path.join(".")),
        )),
    }
}

fn value_at<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut current = value;
    for segment in path {
        current = current.get(*segment)?;
    }
    Some(current)
}

fn string_at(value: &Value, path: &[&str]) -> Option<String> {
    value_at(value, path)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

fn bool_at(value: &Value, path: &[&str]) -> Option<bool> {
    value_at(value, path).and_then(Value::as_bool)
}

fn array_at<'a>(value: &'a Value, path: &[&str]) -> Vec<&'a Value> {
    value_at(value, path)
        .and_then(Value::as_array)
        .map(|values| values.iter().collect())
        .unwrap_or_default()
}

fn array_strings_at(value: &Value, path: &[&str]) -> Vec<String> {
    value_at(value, path)
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn string_field(value: &Value, field: &str) -> Option<String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

fn error(code: &'static str, message: String) -> HandoffFinding {
    HandoffFinding {
        severity: FindingSeverity::Error,
        code,
        message,
    }
}

fn warning(code: &'static str, message: String) -> HandoffFinding {
    HandoffFinding {
        severity: FindingSeverity::Warning,
        code,
        message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_payload() -> String {
        r#"{
          "format":"canvas.bolt_handoff.v0.1",
          "kind":"planning_request",
          "source":{"product":"rumble-canvas","workspace_id":"w","handoff_id":"h","created_by":"a","created_at":"2026-06-30T00:00:00Z"},
          "package":{"package_id":"p","version":"0.1.0","package_hash":"sha256:0000000000000000000000000000000000000000000000000000000000000000","artifact_reference_id":null,"items":[{"section_id":"s","revision_id":"r"}]},
          "planning_scope":{"mode":"full_package","target_objects":[],"excluded_objects":[],"goal":"Plan only"},
          "spec_context":{},
          "traceability_links":[{"source_type":"journey","source_id":"j","target_type":"action","target_id":"a","relation_type":"implements"}],
          "active_waivers":[],
          "open_questions":[],
          "risks":[],
          "capability_candidates":[],
          "requested_outputs":["implementation_plan"],
          "execution_policy":{"planning_only":true,"allow_execution":false,"requires_human_approval_for_execution":true}
        }"#
        .to_string()
    }

    #[test]
    fn accepts_valid_planning_only_payload() {
        let report = validate_str(&valid_payload()).unwrap();
        assert!(report.is_valid(), "{:#?}", report.findings);
    }

    #[test]
    fn rejects_execution_enabled_payload() {
        let payload =
            valid_payload().replace("\"allow_execution\":false", "\"allow_execution\":true");
        let report = validate_str(&payload).unwrap();
        assert!(!report.is_valid());
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.code == "execution_policy_forbidden")
        );
    }

    #[test]
    fn rejects_missing_traceability() {
        let payload = valid_payload().replace(
            "\"traceability_links\":[{\"source_type\":\"journey\",\"source_id\":\"j\",\"target_type\":\"action\",\"target_id\":\"a\",\"relation_type\":\"implements\"}]",
            "\"traceability_links\":[]",
        );
        let report = validate_str(&payload).unwrap();
        assert!(!report.is_valid());
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.code == "missing_traceability_links")
        );
    }

    #[test]
    fn rejects_artifact_reference_without_integrity_hash() {
        let payload = valid_payload().replace(
            "\"artifact_reference_id\":null",
            "\"artifact_reference_id\":\"artifact:package-demo\"",
        );
        let report = validate_str(&payload).unwrap();
        assert!(!report.is_valid());
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.code == "artifact_integrity_failed")
        );
    }

    #[test]
    fn rejects_sovereignty_violation() {
        let payload = valid_payload().replace(
            "\"requested_outputs\":[\"implementation_plan\"]",
            "\"constraints\":{\"sovereignty\":\"violated: mandatory US SaaS for core truth\",\"requires_external_saas\":true},\"requested_outputs\":[\"implementation_plan\"]",
        );
        let report = validate_str(&payload).unwrap();
        assert!(!report.is_valid());
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.code == "sovereignty_policy_violation")
        );
    }

    #[test]
    fn rejects_handoff_hash_conflict_when_prior_hash_differs() {
        let payload = valid_payload().replace(
            "\"requested_outputs\":[\"implementation_plan\"]",
            "\"idempotency\":{\"prior_payload_hash\":\"sha256:1111111111111111111111111111111111111111111111111111111111111111\",\"payload_hash\":\"sha256:2222222222222222222222222222222222222222222222222222222222222222\"},\"requested_outputs\":[\"implementation_plan\"]",
        );
        let report = validate_str(&payload).unwrap();
        assert!(!report.is_valid());
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.code == "handoff_hash_conflict")
        );
    }

    fn with_wrench_evidence_status(status: &str) -> String {
        valid_payload().replace(
            "\"execution_policy\":",
            &format!(
                "\"evidence_refs\":[{{\"kind\":\"wrench.evidence_report.v0.1\",\"producer\":\"wrench-inspect\",\"hash\":\"sha256:3333333333333333333333333333333333333333333333333333333333333333\",\"status\":\"{status}\",\"summary\":\"Portal evidence report\"}}],\"execution_policy\":"
            ),
        )
    }

    #[test]
    fn accepts_passed_wrench_evidence_ref() {
        let payload = with_wrench_evidence_status("passed");
        let report = validate_str(&payload).unwrap();
        assert!(report.is_valid(), "{:#?}", report.findings);

        let plan = plan_from_payload(&payload);
        let gate = plan
            .gates
            .iter()
            .find(|g| g.code == "wrench_report_passed")
            .expect("wrench_report_passed gate should exist");
        assert_eq!(gate.status, "pass");
        assert!(!gate.detail.contains("skeleton"));
    }

    #[test]
    fn rejects_failed_wrench_evidence_ref() {
        let payload = with_wrench_evidence_status("failed");
        let report = validate_str(&payload).unwrap();
        assert!(!report.is_valid());
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.code == "wrench_evidence_not_passing")
        );

        let plan = plan_from_payload(&payload);
        let gate = plan
            .gates
            .iter()
            .find(|g| g.code == "wrench_report_passed")
            .expect("wrench_report_passed gate should exist");
        assert_eq!(gate.status, "blocked");
        assert!(plan.tasks.is_empty());
    }

    #[test]
    fn rejects_quarantined_wrench_evidence_ref() {
        let payload = with_wrench_evidence_status("quarantined");
        let report = validate_str(&payload).unwrap();
        assert!(!report.is_valid());
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.code == "wrench_evidence_not_passing")
        );
    }

    fn plan_from_payload(payload_str: &str) -> DryRunPlan {
        let payload: Value = serde_json::from_str(payload_str).unwrap();
        let report = validate_payload(&payload);
        dry_run_plan_from_report_and_payload(&report, &payload)
    }

    #[test]
    fn human_approval_checkpoint_passes_when_future_execution_requires_approval() {
        let payload = valid_payload();
        let plan = plan_from_payload(&payload);
        let gate = plan
            .gates
            .iter()
            .find(|g| g.code == "human_approval_checkpoint")
            .expect("human_approval_checkpoint gate should exist");
        assert_eq!(gate.status, "pass");
        assert!(!gate.detail.contains("skeleton"));
    }

    #[test]
    fn human_approval_checkpoint_blocks_when_execution_is_requested() {
        let payload =
            valid_payload().replace("\"allow_execution\":false", "\"allow_execution\":true");
        let plan = plan_from_payload(&payload);
        let gate = plan
            .gates
            .iter()
            .find(|g| g.code == "human_approval_checkpoint")
            .expect("human_approval_checkpoint gate should exist");
        assert_eq!(gate.status, "blocked");
    }

    #[test]
    fn gates_traceability_present_passes_when_links_exist() {
        let payload = valid_payload();
        let plan = plan_from_payload(&payload);
        let gate = plan
            .gates
            .iter()
            .find(|g| g.code == "traceability_present")
            .expect("traceability_present gate should exist");
        assert_eq!(
            gate.status, "pass",
            "traceability gate should pass when links are present"
        );
    }

    #[test]
    fn gates_traceability_present_fails_when_links_empty() {
        let payload = valid_payload().replace(
            "\"traceability_links\":[{\"source_type\":\"journey\",\"source_id\":\"j\",\"target_type\":\"action\",\"target_id\":\"a\",\"relation_type\":\"implements\"}]",
            "\"traceability_links\":[]",
        );
        let plan = plan_from_payload(&payload);
        let gate = plan
            .gates
            .iter()
            .find(|g| g.code == "traceability_present")
            .expect("traceability_present gate should exist");
        assert_eq!(
            gate.status, "blocked",
            "traceability gate should fail when links are empty"
        );
    }

    #[test]
    fn gates_vary_by_handoff_content() {
        // Fixture 1: valid payload with traceability
        let payload1 = valid_payload();
        let plan1 = plan_from_payload(&payload1);

        // Fixture 2: same but missing traceability
        let payload2 = valid_payload().replace(
            "\"traceability_links\":[{\"source_type\":\"journey\",\"source_id\":\"j\",\"target_type\":\"action\",\"target_id\":\"a\",\"relation_type\":\"implements\"}]",
            "\"traceability_links\":[]",
        );
        let plan2 = plan_from_payload(&payload2);

        // The two plans must have different gate sets
        assert_ne!(
            plan1.gates, plan2.gates,
            "gates must vary when handoff content differs"
        );
    }

    #[test]
    fn approval_key_state_refuses_invalid_expiry_timestamp() {
        let key = ApprovalKeyRef {
            public_key_ref: "human-operator-demo-key-01".to_string(),
            public_key: "ed25519:0000000000000000000000000000000000000000000000000000000000000000"
                .to_string(),
            state: "active".to_string(),
            not_before: None,
            expires_at: "not-a-timestamp".to_string(),
            rotated_from: None,
            rotated_to: None,
            revoked_at: None,
        };
        let now = parse_rfc3339_timestamp("2026-07-02T00:00:00Z").unwrap();

        assert_eq!(key.effective_state(now), "invalid_time");
    }
}
