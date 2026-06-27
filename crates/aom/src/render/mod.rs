//! Per-agent rendering. An [`Adapter`] turns the ordered domains of a profile
//! into one agent's output text. Phase 1 ships the `universal` adapter (AGENTS.md).
//!
//! Generated text is pristine: no tool sentinel is injected (ADR-0004); the
//! sentinel lives out-of-band in the lockfile.

pub mod universal;

use crate::ir::ResolvedDomain;

/// Renders ordered domains into one agent's configuration text.
pub trait Adapter {
    /// Stable adapter id as referenced by a target's `adapter = "..."`.
    fn id(&self) -> &'static str;

    /// Render the ordered domains into deterministic output text.
    fn render(&self, domains: &[&ResolvedDomain]) -> String;
}

/// Look up an adapter by id, or `None` if unknown.
pub fn adapter_for(id: &str) -> Option<Box<dyn Adapter>> {
    match id {
        "universal" => Some(Box::new(universal::Universal)),
        _ => None,
    }
}
