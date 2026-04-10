//! The embedded content library (ADR: embedded-content-library): neutral, reusable instruction
//! domains compiled into the binary via `include_str!`. No filesystem, no
//! network — one static binary carries its batteries.

use crate::config::schema::Domain;
use crate::error::{Error, Result};

/// One built-in domain.
struct Builtin {
    name: &'static str,
    priority: i64,
    description: &'static str,
    content: &'static str,
}

/// The catalog. Ordered by descending priority for a stable `library list`.
const BUILTINS: &[Builtin] = &[
    Builtin {
        name: "security-baseline",
        priority: 90,
        description: "Validate input at the boundary; never log secrets or PII.",
        content: include_str!("../../../content/domains/security-baseline.md"),
    },
    Builtin {
        name: "four-axes",
        priority: 85,
        description: "Decide on Security > Quality > Performance > Completeness.",
        content: include_str!("../../../content/domains/four-axes.md"),
    },
    Builtin {
        name: "code-style",
        priority: 70,
        description: "Explicit over implicit; readability over cleverness.",
        content: include_str!("../../../content/domains/code-style.md"),
    },
    Builtin {
        name: "tdd",
        priority: 65,
        description: "Tests for non-trivial logic; tests are the spec.",
        content: include_str!("../../../content/domains/tdd.md"),
    },
    Builtin {
        name: "agent-behavior",
        priority: 60,
        description: "Read real state before acting; verify before prescribing.",
        content: include_str!("../../../content/domains/agent-behavior.md"),
    },
    Builtin {
        name: "anti-gold-plating",
        priority: 55,
        description: "Build what the task needs, not what it might need.",
        content: include_str!("../../../content/domains/anti-gold-plating.md"),
    },
    Builtin {
        name: "factual-style",
        priority: 50,
        description: "No filler; calibrate uncertainty; the why, not the what.",
        content: include_str!("../../../content/domains/factual-style.md"),
    },
    Builtin {
        name: "response-blocks",
        priority: 45,
        description: "ORIENTATION / PROPOSITION / DELTA / DECISION vocabulary.",
        content: include_str!("../../../content/domains/response-blocks.md"),
    },
];

fn find(name: &str) -> Option<&'static Builtin> {
    BUILTINS.iter().find(|b| b.name == name)
}

/// Materialize a built-in as an inline-content [`Domain`], or error if unknown.
pub fn lookup(name: &str) -> Result<Domain> {
    find(name)
        .map(|b| Domain {
            name: b.name.to_string(),
            priority: b.priority,
            content: Some(b.content.to_string()),
            content_file: None,
            globs: None,
        })
        .ok_or_else(|| Error::UnknownBuiltin {
            name: name.to_string(),
        })
}

/// `(name, priority, description)` for every built-in, for `library list`.
pub fn catalog() -> Vec<(&'static str, i64, &'static str)> {
    BUILTINS
        .iter()
        .map(|b| (b.name, b.priority, b.description))
        .collect()
}

/// The raw content of a built-in, for `library show`.
pub fn content(name: &str) -> Result<&'static str> {
    find(name)
        .map(|b| b.content)
        .ok_or_else(|| Error::UnknownBuiltin {
            name: name.to_string(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_known_builtin_returns_inline_domain() {
        let d = lookup("four-axes").unwrap();
        assert_eq!(d.name, "four-axes");
        assert!(d.content.is_some());
        assert!(d.content_file.is_none());
        assert!(d.content.unwrap().contains("Decision Axes"));
    }

    #[test]
    fn lookup_unknown_builtin_errors() {
        let err = lookup("does-not-exist").unwrap_err();
        assert!(matches!(err, Error::UnknownBuiltin { .. }));
    }

    #[test]
    fn catalog_is_non_empty_with_unique_safe_names() {
        let cat = catalog();
        assert!(!cat.is_empty());
        let mut names: Vec<&str> = cat.iter().map(|(n, _, _)| *n).collect();
        let count = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), count, "built-in names must be unique");
        // Every built-in name must itself be a safe identifier.
        for (name, _, _) in &cat {
            let mut chars = name.chars();
            assert!(matches!(chars.next(), Some(c) if c.is_ascii_alphanumeric()));
            assert!(chars.all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
        }
    }

    #[test]
    fn content_matches_lookup() {
        assert_eq!(
            content("tdd").unwrap(),
            lookup("tdd").unwrap().content.unwrap()
        );
    }

    #[test]
    fn every_builtin_has_non_empty_content() {
        for (name, _, _) in catalog() {
            let c = content(name).unwrap();
            assert!(!c.trim().is_empty(), "builtin `{name}` has empty content");
        }
    }

    #[test]
    fn catalog_is_ordered_by_descending_priority() {
        let cat = catalog();
        for w in cat.windows(2) {
            assert!(
                w[0].1 >= w[1].1,
                "catalog not sorted by descending priority: {:?} before {:?}",
                w[0],
                w[1]
            );
        }
    }
}
