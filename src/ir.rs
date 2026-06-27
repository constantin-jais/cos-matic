//! The canonical intermediate representation.
//!
//! [`build`] turns resolved domain sources + profiles + targets into a validated
//! [`ConfigTree`]: domain content is loaded (inline or from a Markdown file), the
//! `content` / `content_file` exclusivity is enforced, and every profile→domain
//! and target→profile reference is checked. Fail-fast with a pointed error.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::config::schema::{Profile, Target};
use crate::error::{Error, Result};
use crate::paths::resolve_within;
use crate::resolve::DomainSource;

/// A domain with its content materialized.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedDomain {
    pub name: String,
    pub priority: i64,
    pub content: String,
}

/// The validated, content-loaded configuration.
#[derive(Debug, Clone)]
pub struct ConfigTree {
    pub domains: Vec<ResolvedDomain>,
    pub profiles: Vec<Profile>,
    pub targets: Vec<Target>,
}

impl ConfigTree {
    pub fn domain(&self, name: &str) -> Option<&ResolvedDomain> {
        self.domains.iter().find(|d| d.name == name)
    }

    pub fn profile(&self, name: &str) -> Option<&Profile> {
        self.profiles.iter().find(|p| p.name == name)
    }
}

/// Build and validate the [`ConfigTree`].
pub fn build(
    project_root: &Path,
    sources: Vec<DomainSource>,
    profiles: Vec<Profile>,
    targets: Vec<Target>,
) -> Result<ConfigTree> {
    let mut domains = Vec::with_capacity(sources.len());
    for src in sources {
        let d = src.domain;
        let content = match (d.content.as_ref(), d.content_file.as_ref()) {
            (Some(inline), None) => inline.clone(),
            (None, Some(rel)) => load_content(project_root, &src.base_dir, &d.name, rel)?,
            // neither or both -> ambiguous
            _ => return Err(Error::DomainContent { name: d.name }),
        };
        domains.push(ResolvedDomain {
            name: d.name,
            priority: d.priority,
            content,
        });
    }

    let tree = ConfigTree {
        domains,
        profiles,
        targets,
    };

    // Duplicate names would silently shadow each other (lookups take the first
    // match), so reject them outright rather than guess intent.
    check_unique(tree.domains.iter().map(|d| d.name.as_str()), "domain")?;
    check_unique(tree.profiles.iter().map(|p| p.name.as_str()), "profile")?;
    check_unique(tree.targets.iter().map(|t| t.name.as_str()), "target")?;

    // Two targets writing the same file would clobber each other within one run.
    let mut outputs: HashMap<&str, &str> = HashMap::new();
    for t in &tree.targets {
        if let Some(of) = &t.output_file
            && let Some(first) = outputs.insert(of.as_str(), t.name.as_str())
        {
            return Err(Error::DuplicateOutput {
                output_file: of.clone(),
                first: first.to_string(),
                second: t.name.clone(),
            });
        }
    }

    for p in &tree.profiles {
        for dn in &p.domains {
            if tree.domain(dn).is_none() {
                return Err(Error::UnknownDomain {
                    profile: p.name.clone(),
                    domain: dn.clone(),
                });
            }
        }
    }
    for t in &tree.targets {
        if tree.profile(&t.profile).is_none() {
            return Err(Error::UnknownProfile {
                target: t.name.clone(),
                profile: t.profile.clone(),
            });
        }
    }

    Ok(tree)
}

/// Error if any name appears twice. `kind` labels the diagnostic.
fn check_unique<'a>(names: impl Iterator<Item = &'a str>, kind: &str) -> Result<()> {
    let mut seen = HashSet::new();
    for name in names {
        if !seen.insert(name) {
            return Err(Error::DuplicateName {
                kind: kind.to_string(),
                name: name.to_string(),
            });
        }
    }
    Ok(())
}

fn load_content(project_root: &Path, base: &Path, domain: &str, rel: &str) -> Result<String> {
    let path = resolve_within(project_root, base, rel)?;
    std::fs::read_to_string(&path).map_err(|source| {
        if source.kind() == std::io::ErrorKind::NotFound {
            Error::MissingContentFile {
                name: domain.to_string(),
                path: rel.to_string(),
            }
        } else {
            Error::Io {
                path: rel.to_string(),
                source,
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::schema::Domain;
    use std::path::PathBuf;

    fn src(domain: Domain, base: &Path) -> DomainSource {
        DomainSource {
            domain,
            base_dir: base.to_path_buf(),
        }
    }

    fn domain(name: &str, content: Option<&str>, content_file: Option<&str>) -> Domain {
        Domain {
            name: name.to_string(),
            priority: 0,
            content: content.map(str::to_string),
            content_file: content_file.map(str::to_string),
        }
    }

    #[test]
    fn loads_inline_content() {
        let root = PathBuf::from("/proj");
        let tree = build(
            &root,
            vec![src(domain("a", Some("hello"), None), &root)],
            vec![],
            vec![],
        )
        .unwrap();
        assert_eq!(tree.domain("a").unwrap().content, "hello");
    }

    #[test]
    fn loads_content_from_file_relative_to_manifest() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::create_dir(root.join("domains")).unwrap();
        std::fs::write(root.join("domains/a.md"), "from file").unwrap();
        let tree = build(
            root,
            vec![src(domain("a", None, Some("domains/a.md")), root)],
            vec![],
            vec![],
        )
        .unwrap();
        assert_eq!(tree.domain("a").unwrap().content, "from file");
    }

    #[test]
    fn rejects_domain_with_both_content_and_file() {
        let root = PathBuf::from("/proj");
        let err = build(
            &root,
            vec![src(domain("a", Some("x"), Some("a.md")), &root)],
            vec![],
            vec![],
        )
        .unwrap_err();
        assert!(matches!(err, Error::DomainContent { .. }));
    }

    #[test]
    fn rejects_domain_with_neither_content_nor_file() {
        let root = PathBuf::from("/proj");
        let err = build(
            &root,
            vec![src(domain("a", None, None), &root)],
            vec![],
            vec![],
        )
        .unwrap_err();
        assert!(matches!(err, Error::DomainContent { .. }));
    }

    #[test]
    fn reports_missing_content_file() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let err = build(
            root,
            vec![src(domain("a", None, Some("nope.md")), root)],
            vec![],
            vec![],
        )
        .unwrap_err();
        assert!(matches!(err, Error::MissingContentFile { .. }));
    }

    #[test]
    fn rejects_profile_referencing_unknown_domain() {
        let root = PathBuf::from("/proj");
        let err = build(
            &root,
            vec![src(domain("a", Some("x"), None), &root)],
            vec![Profile {
                name: "default".into(),
                domains: vec!["missing".into()],
            }],
            vec![],
        )
        .unwrap_err();
        assert!(matches!(err, Error::UnknownDomain { .. }));
    }

    #[test]
    fn rejects_duplicate_domain_names() {
        let root = PathBuf::from("/proj");
        let err = build(
            &root,
            vec![
                src(domain("a", Some("x"), None), &root),
                src(domain("a", Some("y"), None), &root),
            ],
            vec![],
            vec![],
        )
        .unwrap_err();
        assert!(matches!(err, Error::DuplicateName { .. }));
    }

    #[test]
    fn rejects_two_targets_writing_the_same_file() {
        let root = PathBuf::from("/proj");
        let target = |name: &str| Target {
            name: name.into(),
            adapter: "universal".into(),
            output_file: Some("AGENTS.md".into()),
            profile: "p".into(),
        };
        let err = build(
            &root,
            vec![src(domain("a", Some("x"), None), &root)],
            vec![Profile {
                name: "p".into(),
                domains: vec!["a".into()],
            }],
            vec![target("t1"), target("t2")],
        )
        .unwrap_err();
        assert!(matches!(err, Error::DuplicateOutput { .. }));
    }

    #[test]
    fn reports_content_file_that_is_a_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::create_dir(root.join("a_dir")).unwrap();
        let err = build(
            root,
            vec![src(domain("a", None, Some("a_dir")), root)],
            vec![],
            vec![],
        )
        .unwrap_err();
        // Reading a directory as a string is an IO error, not "not found".
        assert!(matches!(err, Error::Io { .. }));
    }

    #[test]
    fn reports_non_utf8_content_file() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::write(root.join("bin.md"), [0xff, 0xfe, 0x00, 0x9f]).unwrap();
        let err = build(
            root,
            vec![src(domain("a", None, Some("bin.md")), root)],
            vec![],
            vec![],
        )
        .unwrap_err();
        assert!(matches!(err, Error::Io { .. }));
    }
}
