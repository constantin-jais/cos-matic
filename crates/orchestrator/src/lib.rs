//! Orchestrator — the agentic CI/CD control loop built on top of the
//! `agent_o_matic` compiler. First autonomous primitive: incident -> issue
//! (idempotent GitHub issue creation), journaled zero-PII. Dispatch (hand-off
//! to a fixer agent) is the next increment. Goals & gates live in the compiler
//! (ADR: goals-safe-declarative-checks).

pub mod forge;
pub mod incident;
