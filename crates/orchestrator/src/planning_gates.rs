//! Evidence-derived planning gates (ADR-0037).
//!
//! Wraps and enhances gate derivation from the core handoff module.
//! Gates are observable verification checkpoints that reflect the actual state
//! of the handoff. Each gate derives its status from handoff content:
//! - pass: requirement fully met.
//! - warn: requirement partially met, incomplete but not blocking.
//! - blocked: requirement violated or safety invariant breached.
//!
//! Skeleton gates are explicitly labeled as `(static)` to indicate they depend
//! on future integrations and do not yet validate real evidence.

use bolt_cos_matic::handoff::{HandoffReport, PlanGate};

/// Derive planning gates from a validated handoff report.
///
/// This function wraps the core gate derivation logic from the handoff module,
/// returning gates that reflect the actual evidence in the handoff.
/// See ADR-0037 for complete semantics.
pub fn derive_gates_from_report(report: &HandoffReport) -> Vec<PlanGate> {
    // Gates are derived in the handoff module directly from the payload.
    // Here we simply re-export the interface for use in the orchestrator layer.
    //
    // Future enhancements may include:
    // - Additional gates for workflow-specific policies.
    // - Scoring or ranking of gates by criticality.
    // - Integration with policy engines or external validators.

    // For P0, gates are derived from the HandoffReport findings and metadata.
    let mut gates = Vec::new();

    // package_approved: check if package data is present
    let package_approved_status = if report.package_id.is_some() {
        "pass"
    } else {
        "blocked"
    };
    gates.push(PlanGate {
        code: "package_approved".to_string(),
        status: package_approved_status.to_string(),
        detail: "package metadata and items are present".to_string(),
    });

    // planning_only_enforced: no execution policy errors → passed
    let planning_enforced = !report
        .findings
        .iter()
        .any(|f| f.code == "execution_policy_forbidden");
    gates.push(PlanGate {
        code: "planning_only_enforced".to_string(),
        status: if planning_enforced { "pass" } else { "blocked" }.to_string(),
        detail: "execution is explicitly forbidden".to_string(),
    });

    // traceability_present: derived from presence of traceability links
    let traceability_status = if report
        .findings
        .iter()
        .any(|f| f.code == "missing_traceability_links")
    {
        "blocked"
    } else {
        "pass"
    };
    gates.push(PlanGate {
        code: "traceability_present".to_string(),
        status: traceability_status.to_string(),
        detail: "traceability links are present".to_string(),
    });

    // blocking_questions_waived_or_absent: check for unwaived blockers
    let has_blocker = report
        .findings
        .iter()
        .any(|f| f.code == "blocking_question_without_waiver");
    gates.push(PlanGate {
        code: "blocking_questions_waived_or_absent".to_string(),
        status: if !has_blocker { "pass" } else { "blocked" }.to_string(),
        detail: "no unwaived blocking question detected".to_string(),
    });

    // high_risks_waived_or_absent: check for unwaived high/critical risks
    let has_high_risk = report
        .findings
        .iter()
        .any(|f| f.code == "high_risk_without_waiver");
    gates.push(PlanGate {
        code: "high_risks_waived_or_absent".to_string(),
        status: if !has_high_risk { "pass" } else { "blocked" }.to_string(),
        detail: "no unwaived high/critical risk detected".to_string(),
    });

    // shared_capability_review_requested: skeleton gate (static, not yet wired)
    gates.push(PlanGate {
        code: "shared_capability_review_requested".to_string(),
        status: "pass".to_string(),
        detail: "skeleton (static): shared capability extraction review can be produced"
            .to_string(),
    });

    // Additional skeleton gates for future integrations.
    gates.push(PlanGate {
        code: "human_approval_checkpoint".to_string(),
        status: "pass".to_string(),
        detail: "skeleton (static): approval workflow not yet integrated".to_string(),
    });
    gates.push(PlanGate {
        code: "artifact_supply_chain_verified".to_string(),
        status: "pass".to_string(),
        detail: "skeleton (static): artifact registry integration pending".to_string(),
    });
    gates.push(PlanGate {
        code: "sovereignty_and_license_audit".to_string(),
        status: "pass".to_string(),
        detail: "skeleton (static): external audit tool integration pending".to_string(),
    });

    gates
}

#[cfg(test)]
mod tests {
    use super::*;
    use bolt_cos_matic::handoff::validate_str;

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
    fn derives_gates_for_healthy_handoff() {
        let report = validate_str(&valid_payload()).unwrap();
        let gates = derive_gates_from_report(&report);

        assert_eq!(gates.len(), 9); // 6 primary + 3 skeleton
        assert!(gates
            .iter()
            .any(|g| g.code == "package_approved" && g.status == "pass"));
        assert!(gates
            .iter()
            .any(|g| g.code == "planning_only_enforced" && g.status == "pass"));
        assert!(gates
            .iter()
            .any(|g| g.code == "traceability_present" && g.status == "pass"));
    }

    #[test]
    fn gates_vary_by_handoff_content() {
        // Healthy handoff: primary gates pass.
        let report_healthy = validate_str(&valid_payload()).unwrap();
        let gates_healthy = derive_gates_from_report(&report_healthy);
        let healthy_primary_status: Vec<_> = gates_healthy
            .iter()
            .filter(|g| !g.detail.contains("skeleton"))
            .map(|g| g.status.clone())
            .collect();

        // Handoff with missing traceability: traceability gate blocks.
        let payload_no_trace = valid_payload().replace(
            "\"traceability_links\":[{\"source_type\":\"journey\",\"source_id\":\"j\",\"target_type\":\"action\",\"target_id\":\"a\",\"relation_type\":\"implements\"}]",
            "\"traceability_links\":[]",
        );
        let report_no_trace = validate_str(&payload_no_trace).unwrap();
        let gates_no_trace = derive_gates_from_report(&report_no_trace);
        let no_trace_primary_status: Vec<_> = gates_no_trace
            .iter()
            .filter(|g| !g.detail.contains("skeleton"))
            .map(|g| g.status.clone())
            .collect();

        // Statuses differ: healthy has all pass, no_trace has at least one blocked.
        assert_ne!(healthy_primary_status, no_trace_primary_status);
        assert!(healthy_primary_status.iter().all(|s| s == "pass"));
        assert!(no_trace_primary_status.iter().any(|s| s == "blocked"));
    }
}
