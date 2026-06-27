//! The end-to-end pipeline: manifest → resolved IR → per-target render →
//! safe-write (or, with `--check`, a drift verification that writes nothing).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::config::parse::parse_file;
use crate::config::schema::Manifest;
use crate::error::{Error, Result};
use crate::goals::{self, GoalOutcome};
use crate::ir::ConfigTree;
use crate::lock::Lockfile;
use crate::render::{RenderInput, adapter_for};
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
    /// Graceful-degradation warnings collected across all targets (ADR-0007).
    pub warnings: Vec<String>,
    /// Outcomes of the declared goals (ADR-0009); hard-gate failures abort the run.
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
pub(crate) fn load_tree(manifest_path: &Path) -> Result<(PathBuf, Manifest, ConfigTree)> {
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

/// Compile the manifest into every declared target.
pub fn run(opts: &Options) -> Result<Report> {
    let (project_root, manifest, tree) = load_tree(&opts.manifest_path)?;

    // Goals run before any write: a failed hard gate aborts without producing
    // output (ADR-0009).
    let goal_outcomes = goals::evaluate(&tree, &manifest.goals)?;
    let failures: Vec<String> = goal_outcomes
        .iter()
        .filter(|o| o.is_blocking_failure())
        .map(|o| format!("  {}: {}", o.check, o.detail))
        .collect();
    if !failures.is_empty() {
        return Err(Error::GoalsFailed { failures });
    }

    let mut lock = Lockfile::load(&project_root)?;
    let mut report = Report {
        goals: goal_outcomes,
        ..Default::default()
    };
    // Guard against two targets resolving to the same output path (case-insensitive,
    // for macOS/Windows): without this, the later write would silently clobber.
    let mut seen_paths: HashSet<String> = HashSet::new();

    for target in &tree.targets {
        let adapter = adapter_for(&target.adapter).ok_or_else(|| Error::UnknownAdapter {
            target: target.name.clone(),
            adapter: target.adapter.clone(),
        })?;

        let profile = tree
            .profile(&target.profile)
            .ok_or_else(|| Error::UnknownProfile {
                target: target.name.clone(),
                profile: target.profile.clone(),
            })?;
        let domains = merge::merge(&tree, profile)?;

        let rendered = adapter.render(&RenderInput {
            domains: &domains,
            target,
        })?;
        report.warnings.extend(rendered.warnings);

        for file in rendered.files {
            if !seen_paths.insert(file.path.to_lowercase()) {
                return Err(Error::DuplicateRenderedPath { path: file.path });
            }
            let action = if opts.check {
                verify_no_drift(&project_root, &file.path, &file.content)?;
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
                path: file.path,
                action,
            });
        }
    }

    if !opts.check {
        lock.save(&project_root)?;
    }

    Ok(report)
}

/// In `--check` mode: the on-disk file must exist and equal the rendered content.
fn verify_no_drift(project_root: &Path, rel_path: &str, content: &str) -> Result<()> {
    let abs = crate::paths::safe_join(project_root, rel_path)?;
    match std::fs::read_to_string(&abs) {
        Ok(on_disk) if on_disk == content => Ok(()),
        // Present but different, or absent entirely: that is genuine drift.
        Ok(_) => Err(Error::Drift {
            path: rel_path.to_string(),
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(Error::Drift {
            path: rel_path.to_string(),
        }),
        // A real IO problem (permissions, etc.) must surface as itself, not as
        // a misleading "drift" that sends the user in circles.
        Err(source) => Err(Error::Io {
            path: rel_path.to_string(),
            source,
        }),
    }
}
