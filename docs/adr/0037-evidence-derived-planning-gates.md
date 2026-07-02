# ADR-0037 — Evidence-derived planning gates

## Status

Proposed (2026-07-02). Governs gate evaluation during orchestration planning.

## Context

The orchestrator's planning phase (`dry_run_plan_file()`) currently hardcodes six gates with status `pass` (see `crates/orchestrator/src/handoff.rs:102–133`). These gates are static: they carry the same evaluation result regardless of the handoff content, evidence context, or the infrastructure being inspected.

The specification (ADR: goals-safe-declarative-checks) declares that gates are **evidence-gated**. A gate should reflect the actual state of the system: did this deploy pass smoke tests? Does this branch have green lint and test results? Has the diff been reviewed? Gates are not scaffolding; they are claims about reality.

The current behavior invites a false belief: that a `pass` gate is earned. In practice, the gates are skeletal; they exist to show the gate structure, not to assert real evidence. This is honest during prototyping but becomes dangerously misleading once the loop runs in a sandbox or production context.

## Decision

1. **Gates must be derived from the handoff `spec_context` and consume Wrench evidence reports by reference.** When `dry_run_plan_file()` evaluates a gate, it:
   - extracts the required evidence category from the gate definition (e.g., `dry_run_evidence_type: "lint"`, `dry_run_evidence_type: "test_coverage"`);
   - retrieves the corresponding Wrench report reference from the handoff metadata;
   - evaluates the gate logic against that evidence (e.g., "is coverage above 80%?");
   - returns a status and a reference to the evidence that supports it.

2. **Until evidence-derived gates are fully implemented, CLI dry-run output must label all gates as `skeleton (static)` to keep claims honest.** The output looks like:

   ```
   Gate: lint
   Status: pass (skeleton — static, not evidence-backed)
   Reference: (none)
   ```

   The word "skeleton" signals to users and auditors that this gate is a placeholder. It is not a green light for production; it is a roadmap of where gates will be.

3. **The cockpit (UI / display / audit) must use consistent wording.** Any interface that surfaces gates must distinguish between evidence-backed gates and skeletal gates. Collapsing them under "gate: pass" erases a critical distinction.

## Consequences

- **Fixture-driven tests must prove gate variance with handoff content.** A test that submits a handoff with high coverage should see `status: pass` on the coverage gate; a handoff with low coverage should see `status: fail` and a reference to the coverage report. The test suite is the contract.
- **The gate evaluation logic is no longer hidden inside `dry_run_plan_file()`.** It moves to a dedicated module (`crates/orchestrator/src/gates/mod.rs`) with typed evidence references and chainable gate logic.
- **Evidence integration is a progressive unlock.** As Wrench reports are added (lint, test, coverage, security scan, branch review), gates can be wired to them one at a time. The skeleton label remains until all gates have evidence.
- **Audit trails record both the gate result and the evidence reference.** A gate that passes because of a Wrench report includes the report URL/ID in the audit; a skeletal gate includes the label `(skeleton)` so auditors know it was not evidence-backed.
- **Breaking change:** any code that reads gate results as authoritative until evidence is wired will need to check for the skeleton label. This is intentional; the breaking change enforces the distinction.

## Implementation notes

The `Gate` struct (currently a simple enum with status) becomes:

```rust
pub struct Gate {
    pub name: &'static str,
    pub evidence_type: Option<EvidenceType>,
    pub required_evidence_reference: Option<Reference>,
    pub status: GateStatus,
    pub evidence_backed: bool,  // false for skeleton gates
}
```

The `evaluate_gate()` function takes the handoff and the Wrench evidence index, looks up the required evidence, and returns a typed result. If evidence is missing and the gate is not yet wired, `evaluate_gate()` returns a status with `evidence_backed: false` to signal the skeletal state.
