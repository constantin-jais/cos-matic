//! Orchestrator — the agentic CI/CD control loop built on top of the
//! `agent_o_matic` compiler. Primitives so far: incident -> issue (idempotent
//! GitHub issue creation), dispatch (a *bounded* hand-off to a fixer agent —
//! isolated branch, single attempt, never merges), automerge (gate-and-merge a
//! branch only on attached green evidence), and deploy (canary -> smoke ->
//! promote-or-auto-rollback) — each inside the binding envelope. Goals & gates
//! live in the compiler (ADR: goals-safe-declarative-checks).

pub mod automerge;
pub mod deploy;
pub mod dispatch;
pub mod forge;
pub mod incident;
