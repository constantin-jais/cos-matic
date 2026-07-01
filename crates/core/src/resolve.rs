//! Resolve `[[includes]]` into a flat, ordered list of domains.
//!
//! Each manifest may include other manifests; their domains are merged in.
//! Cycles are detected (and reported), and a manifest reached twice via
//! different paths (a diamond) contributes its domains only once.
//!
//! Phase 1 scope: includes contribute *domains* only. Profiles and targets come
//! from the root manifest.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::config::parse::parse_file;
use crate::config::schema::{Domain, Manifest};
use crate::error::{Error, Result};
use crate::paths::resolve_within;

/// A domain together with the directory of the manifest that declared it, so
/// its `content_file` can later be resolved relative to that manifest.
#[derive(Debug, Clone)]
pub struct DomainSource {
    pub domain: Domain,
    pub base_dir: PathBuf,
}

/// Resolve the include graph rooted at `root_manifest_path` into an ordered list
/// of domain sources. `project_root` bounds path resolution (no escapes).
pub fn resolve(
    project_root: &Path,
    root_manifest_path: &Path,
    root: &Manifest,
) -> Result<Vec<DomainSource>> {
    let mut out = Vec::new();
    let mut on_stack: Vec<PathBuf> = Vec::new();
    let mut done: HashSet<PathBuf> = HashSet::new();
    // Built-ins are de-duplicated by name across the whole include graph, so
    // pulling the same one from two manifests is a no-op rather than a clash.
    let mut library_added: HashSet<String> = HashSet::new();
    resolve_rec(
        project_root,
        root_manifest_path,
        root,
        &mut out,
        &mut on_stack,
        &mut done,
        &mut library_added,
    )?;
    Ok(out)
}

/// Add a built-in domain once. Built-ins are de-duplicated by *name* (a separate
/// concern from file-include cycle detection, which keys on canonical path): the
/// same built-in pulled from several manifests should be idempotent, not an error.
/// Built-in content is inline, so the `DomainSource.base_dir` is irrelevant here.
fn add_builtin(
    name: &str,
    out: &mut Vec<DomainSource>,
    library_added: &mut HashSet<String>,
) -> Result<()> {
    if !library_added.insert(name.to_string()) {
        return Ok(());
    }
    out.push(DomainSource {
        domain: crate::library::lookup(name)?,
        base_dir: PathBuf::from("."),
    });
    Ok(())
}

fn canonical_key(path: &Path) -> PathBuf {
    // Canonicalize for robust identity; fall back to the given path if the file
    // cannot be canonicalized (it always can here, since we just read it).
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn display_relative(project_root: &Path, path: &Path) -> String {
    path.strip_prefix(project_root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

fn resolve_rec(
    project_root: &Path,
    manifest_path: &Path,
    manifest: &Manifest,
    out: &mut Vec<DomainSource>,
    on_stack: &mut Vec<PathBuf>,
    done: &mut HashSet<PathBuf>,
    library_added: &mut HashSet<String>,
) -> Result<()> {
    let key = canonical_key(manifest_path);

    if on_stack.contains(&key) {
        let mut chain: Vec<String> = on_stack
            .iter()
            .map(|p| display_relative(project_root, p))
            .collect();
        chain.push(display_relative(project_root, &key));
        return Err(Error::IncludeCycle {
            chain: chain.join(" -> "),
        });
    }
    if done.contains(&key) {
        return Ok(());
    }

    on_stack.push(key.clone());

    let base_dir = manifest_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));

    for domain in &manifest.domains {
        out.push(DomainSource {
            domain: domain.clone(),
            base_dir: base_dir.clone(),
        });
    }

    // `builtins = [...]` is sugar for `library://<name>` includes (ADR: embedded-content-library).
    for name in &manifest.package.builtins {
        add_builtin(name, out, library_added)?;
    }

    for include in &manifest.includes {
        if let Some(name) = include.path.strip_prefix("library://") {
            add_builtin(name, out, library_added)?;
            continue;
        }
        let inc_path = resolve_within(project_root, &base_dir, &include.path)?;
        let display = display_relative(project_root, &inc_path);
        let inc_manifest = parse_file(&inc_path, &display)?;
        resolve_rec(
            project_root,
            &inc_path,
            &inc_manifest,
            out,
            on_stack,
            done,
            library_added,
        )?;
    }

    on_stack.pop();
    done.insert(key);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::parse::parse_str;
    use std::fs;

    fn write(dir: &Path, name: &str, body: &str) -> PathBuf {
        let p = dir.join(name);
        fs::write(&p, body).unwrap();
        p
    }

    #[test]
    fn flattens_includes_in_order() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(
            root,
            "lib.toml",
            "[package]\nname=\"lib\"\n[[domains]]\nname=\"b\"\ncontent=\"B\"\n",
        );
        let root_path = write(
            root,
            "harness.toml",
            "[package]\nname=\"x\"\n[[domains]]\nname=\"a\"\ncontent=\"A\"\n[[includes]]\npath=\"lib.toml\"\n",
        );
        let m = parse_str("harness.toml", &fs::read_to_string(&root_path).unwrap()).unwrap();
        let domains = resolve(root, &root_path, &m).unwrap();
        let names: Vec<&str> = domains.iter().map(|d| d.domain.name.as_str()).collect();
        assert_eq!(names, vec!["a", "b"]);
    }

    #[test]
    fn detects_include_cycle() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(
            root,
            "a.toml",
            "[package]\nname=\"a\"\n[[includes]]\npath=\"b.toml\"\n",
        );
        write(
            root,
            "b.toml",
            "[package]\nname=\"b\"\n[[includes]]\npath=\"a.toml\"\n",
        );
        let a_path = root.join("a.toml");
        let m = parse_str("a.toml", &fs::read_to_string(&a_path).unwrap()).unwrap();
        let err = resolve(root, &a_path, &m).unwrap_err();
        assert!(matches!(err, Error::IncludeCycle { .. }), "got {err:?}");
    }

    #[test]
    fn diamond_include_contributes_once() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(
            root,
            "d.toml",
            "[package]\nname=\"d\"\n[[domains]]\nname=\"d\"\ncontent=\"D\"\n",
        );
        write(
            root,
            "b.toml",
            "[package]\nname=\"b\"\n[[includes]]\npath=\"d.toml\"\n",
        );
        write(
            root,
            "c.toml",
            "[package]\nname=\"c\"\n[[includes]]\npath=\"d.toml\"\n",
        );
        let a_path = write(
            root,
            "a.toml",
            "[package]\nname=\"a\"\n[[includes]]\npath=\"b.toml\"\n[[includes]]\npath=\"c.toml\"\n",
        );
        let m = parse_str("a.toml", &fs::read_to_string(&a_path).unwrap()).unwrap();
        let domains = resolve(root, &a_path, &m).unwrap();
        let count = domains.iter().filter(|d| d.domain.name == "d").count();
        assert_eq!(count, 1, "diamond include should add domain `d` once");
    }

    #[test]
    fn detects_self_referential_include() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let a_path = write(
            root,
            "a.toml",
            "[package]\nname=\"a\"\n[[includes]]\npath=\"a.toml\"\n",
        );
        let m = parse_str("a.toml", &fs::read_to_string(&a_path).unwrap()).unwrap();
        let err = resolve(root, &a_path, &m).unwrap_err();
        assert!(matches!(err, Error::IncludeCycle { .. }), "got {err:?}");
    }

    #[test]
    fn reports_missing_include_file() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let a_path = write(
            root,
            "a.toml",
            "[package]\nname=\"a\"\n[[includes]]\npath=\"missing.toml\"\n",
        );
        let m = parse_str("a.toml", &fs::read_to_string(&a_path).unwrap()).unwrap();
        let err = resolve(root, &a_path, &m).unwrap_err();
        assert!(matches!(err, Error::Io { .. }), "got {err:?}");
    }

    fn resolve_src(root: &Path, body: &str) -> Result<Vec<DomainSource>> {
        let p = write(root, "harness.toml", body);
        let m = parse_str("harness.toml", &fs::read_to_string(&p).unwrap()).unwrap();
        resolve(root, &p, &m)
    }

    #[test]
    fn builtins_are_desugared_into_domains() {
        let tmp = tempfile::tempdir().unwrap();
        let domains = resolve_src(
            tmp.path(),
            "[package]\nname=\"x\"\nbuiltins=[\"four-axes\",\"tdd\"]\n",
        )
        .unwrap();
        let names: Vec<&str> = domains.iter().map(|d| d.domain.name.as_str()).collect();
        assert!(names.contains(&"four-axes"));
        assert!(names.contains(&"tdd"));
    }

    #[test]
    fn library_include_adds_a_builtin() {
        let tmp = tempfile::tempdir().unwrap();
        let domains = resolve_src(
            tmp.path(),
            "[package]\nname=\"x\"\n[[includes]]\npath=\"library://response-blocks\"\n",
        )
        .unwrap();
        assert_eq!(
            domains
                .iter()
                .filter(|d| d.domain.name == "response-blocks")
                .count(),
            1
        );
    }

    #[test]
    fn the_same_builtin_pulled_twice_is_deduplicated() {
        let tmp = tempfile::tempdir().unwrap();
        let domains = resolve_src(
            tmp.path(),
            "[package]\nname=\"x\"\nbuiltins=[\"four-axes\"]\n[[includes]]\npath=\"library://four-axes\"\n",
        )
        .unwrap();
        assert_eq!(
            domains
                .iter()
                .filter(|d| d.domain.name == "four-axes")
                .count(),
            1
        );
    }

    #[test]
    fn unknown_builtin_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let err =
            resolve_src(tmp.path(), "[package]\nname=\"x\"\nbuiltins=[\"nope\"]\n").unwrap_err();
        assert!(matches!(err, Error::UnknownBuiltin { .. }));
    }

    #[test]
    fn builtins_on_an_included_manifest_are_resolved() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(
            root,
            "lib.toml",
            "[package]\nname=\"lib\"\nbuiltins=[\"four-axes\"]\n",
        );
        let p = write(
            root,
            "harness.toml",
            "[package]\nname=\"x\"\n[[includes]]\npath=\"lib.toml\"\n",
        );
        let m = parse_str("harness.toml", &fs::read_to_string(&p).unwrap()).unwrap();
        let domains = resolve(root, &p, &m).unwrap();
        assert!(domains.iter().any(|d| d.domain.name == "four-axes"));
    }
}
