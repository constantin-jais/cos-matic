//! Per-agent rendering.
//!
//! An [`Adapter`] turns the ordered domains of a profile into one or more output
//! files for a target agent (ADR: adapter-output-model: output is a *set of files*, not a single
//! string). Adapters declare which gateable [`Feature`]s they support; the engine
//! and adapters degrade gracefully — with a recorded warning — when a domain asks
//! for a feature the target cannot honor (ADR: feature-gating-graceful-degradation).

pub mod claude;
pub mod cursor;
pub mod universal;

use crate::config::schema::Target;
use crate::error::{Error, Result};
use crate::ir::ResolvedDomain;

/// A gateable capability some adapters support and others do not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Feature {
    /// Per-file glob activation (Cursor `.mdc` `globs`).
    GlobActivation,
    /// Claude subagents (`.claude/agents/`).
    Subagents,
    /// Claude skills (`.claude/skills/`).
    Skills,
    /// Claude hooks (`.claude/settings.json`).
    Hooks,
}

impl Feature {
    pub fn label(self) -> &'static str {
        match self {
            Feature::GlobActivation => "glob activation",
            Feature::Subagents => "subagents",
            Feature::Skills => "skills",
            Feature::Hooks => "hooks",
        }
    }
}

/// One file an adapter wants written, at a repo-relative path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedFile {
    pub path: String,
    pub content: String,
}

/// Everything an adapter needs to render a target.
pub struct RenderInput<'a> {
    /// Domains already selected and ordered by priority.
    pub domains: &'a [&'a ResolvedDomain],
    pub target: &'a Target,
}

/// The result of rendering: files to write plus any degradation warnings.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RenderOutput {
    pub files: Vec<RenderedFile>,
    pub warnings: Vec<String>,
}

/// Renders ordered domains into one agent's configuration files.
pub trait Adapter {
    /// Stable adapter id as referenced by a target's `adapter = "..."`.
    fn id(&self) -> &'static str;

    /// Whether this adapter can honor `feature`.
    fn supports(&self, feature: Feature) -> bool;

    /// Render the target's files (and any warnings).
    fn render(&self, input: &RenderInput) -> Result<RenderOutput>;
}

/// Look up an adapter by id, or `None` if unknown.
pub fn adapter_for(id: &str) -> Option<Box<dyn Adapter>> {
    match id {
        "universal" => Some(Box::new(universal::Universal)),
        "claude" => Some(Box::new(claude::Claude)),
        "cursor" => Some(Box::new(cursor::Cursor)),
        _ => None,
    }
}

// --- shared helpers ---------------------------------------------------------

/// True when a domain declares a *non-empty* glob set. Centralizes the
/// "`Some(vec![])` means no globs" rule so every adapter agrees.
pub(crate) fn has_globs(domain: &ResolvedDomain) -> bool {
    domain.globs.as_ref().is_some_and(|g| !g.is_empty())
}

/// Concatenate domain contents into one Markdown body: priority order, blank-line
/// separated, empty domains skipped, single trailing newline. Used by the
/// single-file Markdown adapters (universal, claude).
pub(crate) fn concatenate(domains: &[&ResolvedDomain]) -> String {
    let body = domains
        .iter()
        .map(|d| d.content.trim_end())
        .filter(|c| !c.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    if body.is_empty() {
        String::new()
    } else {
        format!("{body}\n")
    }
}

/// Warnings for any domain that declared a feature this adapter cannot honor.
/// The content is still rendered (unconditionally); only the feature is dropped.
pub(crate) fn degradation_warnings(
    adapter_id: &str,
    domains: &[&ResolvedDomain],
    feature: Feature,
    domain_uses_feature: impl Fn(&ResolvedDomain) -> bool,
) -> Vec<String> {
    domains
        .iter()
        .filter(|d| domain_uses_feature(d))
        .map(|d| {
            format!(
                "adapter `{adapter_id}` does not support {feat}; domain `{name}` applied unconditionally",
                feat = feature.label(),
                name = d.name,
            )
        })
        .collect()
}

/// Require a target's `output_file`, with a clear error otherwise.
pub(crate) fn require_output_file(target: &Target) -> Result<&str> {
    target
        .output_file
        .as_deref()
        .ok_or_else(|| Error::MissingOutput {
            target: target.name.clone(),
        })
}

/// Require a target's `output_dir`, with a clear error otherwise.
pub(crate) fn require_output_dir(target: &Target) -> Result<&str> {
    target
        .output_dir
        .as_deref()
        .ok_or_else(|| Error::MissingOutput {
            target: target.name.clone(),
        })
}
