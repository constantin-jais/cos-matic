//! Orchestrator — the agentic CI/CD control loop built on top of the
//! `bolt_cos_matic` compiler. Primitives so far: incident -> issue (idempotent
//! GitHub issue creation), dispatch (a *bounded* hand-off to a fixer agent —
//! isolated branch, single attempt, never merges), automerge (gate-and-merge a
//! branch only on attached green evidence), and deploy (canary -> smoke ->
//! promote-or-auto-rollback). `pipeline` chains them end-to-end (dispatch ->
//! automerge -> deploy) under one global envelope that short-circuits at the
//! first stop. Each step stays inside the binding envelope; goals & gates live
//! in the compiler (ADR: goals-safe-declarative-checks).

pub mod automerge;
pub mod deploy;
pub mod dispatch;
pub mod forge;
pub mod incident;
pub mod pipeline;
