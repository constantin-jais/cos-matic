# ADR-0037 — Evidence-derived planning gates

## Status

Accepted (2026-07-02).

## Context

Today, `dry_run_plan_file()` generates a fixed set of 6 planning gates with
`status=pass`, regardless of the handoff content. Gates are a control mechanism
that shows what the orchestrator verified, what risks remain, and whether
conditions are met for deployment.

Gates should be derived from the evidence: the actual properties observed in the
handoff and Wrench reports. A gate status is not an opinion; it is a fact about
what the evidence says.

Examples:

- **Traceability gate**: "Are there traceability links in the handoff?" → pass
  if the `spec_context` includes non-empty `traceability_links`; fail otherwise.
- **Waiver gate per risk level**: For each high or critical risk in the Wrench
  report, is there a corresponding waiver in the handoff? → pass if all risks
  have waivers; fail otherwise.
- **Completeness gate**: Is the spec_context syntactically and semantically valid?
  → pass if deserialization and validation succeed; fail or warn otherwise.

Gates encode the invariants that the orchestrator enforces before acting. Today
they are hardcoded as "pass"; they should reflect reality.

## Decision

Redesign gate generation to derive status from observable handoff properties.

### Gate types

1. **Evidence-derived gates** — status depends on content:
   - Traceability: presence of links in `spec_context.traceability_links`.
   - Completeness: spec_context validates without error.
   - Waiver gates: for each high/critical risk, at least one matching waiver
     exists.

2. **Skeleton gates** (temporary, static):
   - Gates that depend on external data not yet integrated (e.g., Wrench risk
     reports, policy store, live system state) are labeled as `skeleton (static)`
     in the gate output.
   - A skeleton gate will be `pass` until the integration point is wired; its
     label is honest about its nature.
   - Example: "compliance_approval" gate depends on a policy service that is not
     yet available — mark it `pass` with label `skeleton (static)` so operators
     see it is not real evidence yet.

### Implementation contract

```rust
pub fn derive_gates(handoff: &Handoff) -> Vec<Gate> {
    // Each gate reads handoff properties and returns a Gate with:
    // - name: string
    // - status: Pass | Fail | Warn
    // - label: Option<string> (e.g., "skeleton (static)")
    // - evidence: string (explains what was checked and why)
}
```

### Test structure

Gates are verified through fixture-driven tests:

- **Fixture 1**: A handoff with all required evidence → gates all pass or warn.
- **Fixture 2**: A handoff missing traceability links → traceability gate fails.
- **Fixture 3**: A handoff with high-risk items but no waivers → waiver gate
  fails.
- Each fixture produces a different gate set, proving gates vary by content.

## Consequences

- Gates now serve as the observable verification contract between the orchestrator
  and its operators.
- A failing gate is not a lie (e.g., "pass" when the link is missing). It is the
  artifact of missing evidence.
- The gate output is auditable: each gate includes evidence explaining what was
  checked.
- Gates can evolve as more evidence sources are integrated (Wrench reports, KMS
  approvals, live system checks) without changing the gate contract.
- Skeleton gates are honest about what is not yet wired.

## Implementation notes

`crates/orchestrator/src/planning_gates.rs` defines the gate struct, the derive
function, and evidence-collection helpers.

The Wrench risk report integration is not yet in scope; when it arrives, risk
gates will read from that source. Until then, risk gates remain skeleton.

## Non-goals

- Does not implement human approval loops or policy evaluations.
- Does not integrate with external policy engines.
- Does not implement scoring or soft-gating (gates are hard: pass/fail/warn).
