//! The end-to-end pipeline: manifest → resolved IR → per-target render →
//! safe-write (or, with `--check`, a drift verification that writes nothing).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::config::parse::parse_file;
use crate::config::schema::{Goal, Manifest};
use crate::error::{Error, Result};
use crate::goals::{self, GoalOutcome};
use crate::ir::ConfigTree;
use crate::lock::Lockfile;
use crate::render::{Feature, RenderInput, RenderedFile, adapter_for};
use crate::safe_write::{self, WriteAction};
use crate::{ir, merge, resolve};

/// How a single output file ended up after a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Created,
    Updated,
    Unchanged,
}

impl Action {
    pub fn label(self) -> &'static str {
        match self {
            Action::Created => "created",
            Action::Updated => "updated",
            Action::Unchanged => "unchanged",
        }
    }
}

impl From<WriteAction> for Action {
    fn from(w: WriteAction) -> Self {
        match w {
            WriteAction::Created => Action::Created,
            WriteAction::Updated => Action::Updated,
            WriteAction::Unchanged => Action::Unchanged,
        }
    }
}

/// One line of the run report.
#[derive(Debug, Clone)]
pub struct FileReport {
    pub path: String,
    pub action: Action,
}

/// Outcome of a generate run.
#[derive(Debug, Clone, Default)]
pub struct Report {
    pub files: Vec<FileReport>,
    /// Graceful-degradation warnings collected across all targets (ADR: feature-gating-graceful-degradation).
    pub warnings: Vec<String>,
    /// Outcomes of the declared goals (ADR: goals-safe-declarative-checks); hard-gate failures abort the run.
    pub goals: Vec<GoalOutcome>,
}

/// The pure result of compiling a [`ConfigTree`]: the files to write (path +
/// content), degradation warnings, and goal outcomes — *no I/O performed*. The
/// caller decides what to do with the files (safe-write, drift-check, or hand
/// them to a WASM/FFI host). See [`compile`].
#[derive(Debug, Clone, Default)]
pub struct Compilation {
    pub files: Vec<RenderedFile>,
    pub warnings: Vec<String>,
    pub goals: Vec<GoalOutcome>,
}

/// Inputs to a generate run.
#[derive(Debug, Clone)]
pub struct Options {
    /// Path to the root manifest (e.g. `harness.toml`).
    pub manifest_path: PathBuf,
    /// Verify outputs are up to date without writing anything (CI gate).
    pub check: bool,
    /// Overwrite files that were hand-edited since the tool last wrote them.
    pub force: bool,
}

/// Parse, resolve, and validate the manifest into a `ConfigTree`. Shared by
/// `generate` and the `goals` command.
pub fn load_tree(manifest_path: &Path) -> Result<(PathBuf, Manifest, ConfigTree)> {
    let display = manifest_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "harness.toml".to_string());

    let abs_manifest = std::fs::canonicalize(manifest_path).map_err(|source| Error::Io {
        path: display.clone(),
        source,
    })?;
    let project_root = abs_manifest
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));

    let manifest = parse_file(&abs_manifest, &display)?;
    let sources = resolve::resolve(&project_root, &abs_manifest, &manifest)?;
    let tree = ir::build(
        &project_root,
        sources,
        manifest.profiles.clone(),
        manifest.targets.clone(),
    )?;
    Ok((project_root, manifest, tree))
}

/// Compile a fully-loaded [`ConfigTree`] into its rendered output files.
///
/// Pure and I/O-free: no filesystem reads or writes, no clock, deterministic.
/// This is the seam that lets the exact same logic run natively, in WASM, or
/// behind an FFI binding (ADR: portability-rust-core-bind-not-reimplement) — the
/// caller loads the tree (I/O) and persists the [`Compilation`] (I/O). The goals
/// gate runs here, before any output, so a blocking hard-gate failure produces
/// no files (ADR: goals-safe-declarative-checks).
pub fn compile(tree: &ConfigTree, goals: &[Goal]) -> Result<Compilation> {
    let goal_outcomes = goals::evaluate(tree, goals)?;
    let failures: Vec<String> = goal_outcomes
        .iter()
        .filter(|o| o.is_blocking_failure())
        .map(|o| format!("  {}: {}", o.check, o.detail))
        .collect();
    if !failures.is_empty() {
        return Err(Error::GoalsFailed { failures });
    }

    let mut out = Compilation {
        goals: goal_outcomes,
        ..Default::default()
    };
    // Guard against two targets resolving to the same output path (case-insensitive,
    // for macOS/Windows): without this, a later write would silently clobber. Caught
    // here, before any I/O, so a duplicate aborts the whole compile atomically.
    let mut seen_paths: HashSet<String> = HashSet::new();

    for target in &tree.targets {
        let adapter = adapter_for(&target.adapter).ok_or_else(|| Error::UnknownAdapter {
            target: target.name.clone(),
            adapter: target.adapter.clone(),
        })?;

        // Tier-2 features declared on a target whose adapter can't honor them are
        // ignored, with a warning (graceful degradation, ADR: feature-gating-graceful-degradation).
        for (declared, feature) in [
            (!target.subagents.is_empty(), Feature::Subagents),
            (!target.skills.is_empty(), Feature::Skills),
            (!target.hooks.is_empty(), Feature::Hooks),
        ] {
            if declared && !adapter.supports(feature) {
                out.warnings.push(format!(
                    "adapter `{}` does not support {}; target `{}` ignores them",
                    target.adapter,
                    feature.label(),
                    target.name,
                ));
            }
        }

        let profile = tree
            .profile(&target.profile)
            .ok_or_else(|| Error::UnknownProfile {
                target: target.name.clone(),
                profile: target.profile.clone(),
            })?;
        let domains = merge::merge(tree, profile)?;

        let rendered = adapter.render(&RenderInput {
            domains: &domains,
            target,
        })?;
        out.warnings.extend(rendered.warnings);

        for file in rendered.files {
            if !seen_paths.insert(file.path.to_lowercase()) {
                return Err(Error::DuplicateRenderedPath { path: file.path });
            }
            out.files.push(file);
        }
    }

    Ok(out)
}

/// Compile the manifest into every declared target and persist the result.
///
/// Thin I/O shell around the pure [`compile`]: load the tree from disk, compile,
/// then either safe-write the rendered files (guarded by the lock) or, with
/// `--check`, verify they are up to date and write nothing.
pub fn run(opts: &Options) -> Result<Report> {
    let (project_root, manifest, tree) = load_tree(&opts.manifest_path)?;

    let compilation = compile(&tree, &manifest.goals)?;

    let mut lock = Lockfile::load(&project_root)?;
    let mut report = Report {
        goals: compilation.goals,
        warnings: compilation.warnings,
        ..Default::default()
    };
    // `--check` reports every drifted file in one pass, not just the first.
    let mut drifted: Vec<String> = Vec::new();

    for file in &compilation.files {
        let action = if opts.check {
            if is_drifted(&project_root, &file.path, &file.content)? {
                drifted.push(file.path.clone());
            }
            Action::Unchanged
        } else {
            safe_write::write(
                &project_root,
                &file.path,
                &file.content,
                &mut lock,
                opts.force,
            )?
            .into()
        };
        report.files.push(FileReport {
            path: file.path.clone(),
            action,
        });
    }

    if opts.check {
        if !drifted.is_empty() {
            return Err(Error::Drift { paths: drifted });
        }
    } else {
        lock.save(&project_root)?;
    }

    Ok(report)
}

/// In `--check` mode: is the on-disk file absent or different from `content`? A
/// real IO error (permissions, etc.) propagates as itself rather than as a
/// misleading "drift" that would send the user in circles.
fn is_drifted(project_root: &Path, rel_path: &str, content: &str) -> Result<bool> {
    let abs = crate::paths::safe_join(project_root, rel_path)?;
    match std::fs::read_to_string(&abs) {
        Ok(on_disk) => Ok(on_disk != content),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(source) => Err(Error::Io {
            path: rel_path.to_string(),
            source,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::schema::{GoalKind, Profile, Target};
    use crate::ir::ResolvedDomain;

    fn domain(name: &str, content: &str) -> ResolvedDomain {
        ResolvedDomain {
            name: name.to_string(),
            priority: 0,
            content: content.to_string(),
            globs: None,
        }
    }

    fn tree_one_target() -> ConfigTree {
        ConfigTree {
            domains: vec![domain("a", "Hello")],
            profiles: vec![Profile {
                name: "p".into(),
                domains: vec!["a".into()],
            }],
            targets: vec![Target {
                name: "t".into(),
                adapter: "universal".into(),
                output_file: Some("AGENTS.md".into()),
                output_dir: None,
                profile: "p".into(),
                subagents: vec![],
                skills: vec![],
                hooks: vec![],
            }],
        }
    }

    #[test]
    fn compile_is_pure_and_renders_declared_targets() {
        // No tempdir, no filesystem: compile() is a pure function of the tree —
        // the property that lets it run in WASM / behind an FFI binding.
        let out = compile(&tree_one_target(), &[]).unwrap();
        assert_eq!(out.files.len(), 1);
        assert_eq!(out.files[0].path, "AGENTS.md");
        assert!(out.files[0].content.contains("Hello"));
        assert!(out.warnings.is_empty());
    }

    #[test]
    fn compile_runs_the_goals_gate_before_rendering() {
        // A failing hard gate aborts with no files (orphan domain → no-dead-domains).
        let mut tree = tree_one_target();
        tree.domains.push(domain("orphan", "x"));
        let goal = Goal {
            kind: GoalKind::HardGate,
            check: "no-dead-domains".into(),
            max: None,
            domains: None,
        };
        let err = compile(&tree, std::slice::from_ref(&goal)).unwrap_err();
        assert!(matches!(err, Error::GoalsFailed { .. }), "got {err:?}");
    }
}
