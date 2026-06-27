//! Orchestrator — the agentic CI/CD control loop built on top of the
//! `agent_o_matic` compiler. A1 adds goals & gates; A3+ add the incident →
//! issue → dispatch loop.

pub mod gate;
pub mod goals;
pub mod incident;
