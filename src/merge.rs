//! Select and order the domains of a profile.
//!
//! Higher `priority` is rendered first. Ties keep the order in which the profile
//! lists them — `sort_by_key` is stable — so output is deterministic.
//!
//! `ir::build` validates every profile→domain reference, so in practice the
//! lookups below never miss. We still return `Result` rather than panic: the
//! library never `unwrap`s on data, and a total function is the clearer teaching
//! example.

use crate::config::schema::Profile;
use crate::error::{Error, Result};
use crate::ir::{ConfigTree, ResolvedDomain};

/// The ordered domains for `profile`, highest priority first.
pub fn merge<'a>(tree: &'a ConfigTree, profile: &Profile) -> Result<Vec<&'a ResolvedDomain>> {
    let mut selected: Vec<&ResolvedDomain> = Vec::with_capacity(profile.domains.len());
    for name in &profile.domains {
        let domain = tree.domain(name).ok_or_else(|| Error::UnknownDomain {
            profile: profile.name.clone(),
            domain: name.clone(),
        })?;
        selected.push(domain);
    }

    // Stable sort: ties keep the profile's declaration order. `Reverse` makes
    // higher priority come first.
    selected.sort_by_key(|d| std::cmp::Reverse(d.priority));
    Ok(selected)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(domains: Vec<(&str, i64)>, selected: Vec<&str>) -> ConfigTree {
        ConfigTree {
            domains: domains
                .into_iter()
                .map(|(name, priority)| ResolvedDomain {
                    name: name.to_string(),
                    priority,
                    content: format!("content-{name}"),
                })
                .collect(),
            profiles: vec![Profile {
                name: "default".into(),
                domains: selected.into_iter().map(str::to_string).collect(),
            }],
            targets: vec![],
        }
    }

    fn names<'a>(domains: &[&'a ResolvedDomain]) -> Vec<&'a str> {
        domains.iter().map(|d| d.name.as_str()).collect()
    }

    #[test]
    fn orders_by_priority_descending() {
        let t = tree(
            vec![("low", 1), ("high", 9), ("mid", 5)],
            vec!["low", "high", "mid"],
        );
        let merged = merge(&t, &t.profiles[0]).unwrap();
        assert_eq!(names(&merged), vec!["high", "mid", "low"]);
    }

    #[test]
    fn ties_keep_declaration_order() {
        let t = tree(vec![("a", 5), ("b", 5), ("c", 5)], vec!["b", "a", "c"]);
        let merged = merge(&t, &t.profiles[0]).unwrap();
        assert_eq!(names(&merged), vec!["b", "a", "c"]);
    }

    #[test]
    fn empty_profile_selects_nothing() {
        let t = tree(vec![("a", 1)], vec![]);
        let merged = merge(&t, &t.profiles[0]).unwrap();
        assert!(merged.is_empty());
    }
}
