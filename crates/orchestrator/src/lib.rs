//! Orchestrator — the agentic CI/CD control loop built on top of the
//! `agent_o_matic` compiler. Phases A1+ add: goals & gates, incident model,
//! GitHub issue bridge, and Claude-Code dispatch. A0 ships only this scaffold.

/// Stable crate identity used by early wiring tests; replaced by real modules in A1.
pub const CRATE_NAME: &str = "orchestrator";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_member_links() {
        assert_eq!(CRATE_NAME, "orchestrator");
    }
}
