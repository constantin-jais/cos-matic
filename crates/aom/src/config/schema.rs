//! The on-disk shape of a `harness.toml` manifest.
//!
//! Forward-compatibility: we deliberately do NOT use `deny_unknown_fields`.
//! serde ignores unknown fields by default, so a manifest carrying not-yet-
//! implemented sections (`[[goals]]`, per-target `subagents = ...`, etc.) parses
//! cleanly today and is simply ignored. This is verified by a test in `parse`.

use serde::Deserialize;

/// A parsed `harness.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Manifest {
    pub package: Package,
    #[serde(default)]
    pub includes: Vec<Include>,
    #[serde(default)]
    pub domains: Vec<Domain>,
    #[serde(default)]
    pub profiles: Vec<Profile>,
    #[serde(default)]
    pub targets: Vec<Target>,
}

/// Project-level metadata. Only `name` is load-bearing in Phase 1.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Package {
    pub name: String,
}

/// A reference to another manifest whose domains are merged in.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Include {
    /// Path to another `*.toml` manifest, relative to the including manifest.
    pub path: String,
}

/// A thematic block of instruction content with an ordering priority.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Domain {
    pub name: String,
    /// Higher priority is rendered first. Defaults to 0.
    #[serde(default)]
    pub priority: i64,
    /// Inline content (mutually exclusive with `content_file`).
    pub content: Option<String>,
    /// Path to a Markdown file holding the content, relative to the manifest.
    pub content_file: Option<String>,
}

/// A named selection of domains for a given audience.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Profile {
    pub name: String,
    #[serde(default)]
    pub domains: Vec<String>,
}

/// An output to generate, bound to a profile and an adapter.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Target {
    pub name: String,
    /// Adapter id, e.g. `"universal"` (→ AGENTS.md). Phase 1 supports `universal`.
    pub adapter: String,
    /// Output file path, relative to the project root.
    pub output_file: Option<String>,
    /// Profile whose domains this target renders.
    pub profile: String,
}
