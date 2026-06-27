//! The end-to-end pipeline: manifest → resolved IR → per-target render →
//! safe-write (or, with `--check`, a drift verification that writes nothing).

use std::path::{Path, PathBuf};

use crate::config::parse::parse_file;
use crate::error::{Error, Result};
use crate::lock::Lockfile;
use crate::render::adapter_for;
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

/// Compile the manifest into every declared target.
pub fn run(opts: &Options) -> Result<Report> {
    let display = opts
        .manifest_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "harness.toml".to_string());

    let abs_manifest = std::fs::canonicalize(&opts.manifest_path).map_err(|source| Error::Io {
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

    let mut lock = Lockfile::load(&project_root)?;
    let mut report = Report::default();

    for target in &tree.targets {
        let adapter = adapter_for(&target.adapter).ok_or_else(|| Error::UnknownAdapter {
            target: target.name.clone(),
            adapter: target.adapter.clone(),
        })?;
        let output_file = target
            .output_file
            .as_ref()
            .ok_or_else(|| Error::MissingOutput {
                target: target.name.clone(),
            })?;

        let domains = merge::merge(&tree, &target.profile);
        let content = adapter.render(&domains);

        let action = if opts.check {
            verify_no_drift(&project_root, output_file, &content)?;
            Action::Unchanged
        } else {
            safe_write::write(&project_root, output_file, &content, &mut lock, opts.force)?.into()
        };

        report.files.push(FileReport {
            path: output_file.clone(),
            action,
        });
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
        _ => Err(Error::Drift {
            path: rel_path.to_string(),
        }),
    }
}
