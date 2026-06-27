//! Select and order the domains of a profile.
//!
//! Higher `priority` is rendered first. Ties keep the order in which the profile
//! lists them — `sort_by` is stable — so output is deterministic.

use crate::ir::{ConfigTree, ResolvedDomain};

/// The ordered domains for `profile_name`, highest priority first.
///
/// Panics if the profile or one of its domains is absent; callers must build the
/// [`ConfigTree`] via [`crate::ir::build`], which validates these references.
pub fn merge<'a>(tree: &'a ConfigTree, profile_name: &str) -> Vec<&'a ResolvedDomain> {
    let profile = tree
        .profile(profile_name)
        .expect("profile reference is validated in ir::build");

    let mut selected: Vec<&ResolvedDomain> = profile
        .domains
        .iter()
        .map(|name| {
            tree.domain(name)
                .expect("domain reference is validated in ir::build")
        })
        .collect();

    // Stable sort: ties keep the profile's declaration order. `Reverse` makes
    // higher priority come first.
    selected.sort_by_key(|d| std::cmp::Reverse(d.priority));
    selected
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::schema::Profile;

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

    #[test]
    fn orders_by_priority_descending() {
        let t = tree(
            vec![("low", 1), ("high", 9), ("mid", 5)],
            vec!["low", "high", "mid"],
        );
        let merged: Vec<&str> = merge(&t, "default")
            .iter()
            .map(|d| d.name.as_str())
            .collect();
        assert_eq!(merged, vec!["high", "mid", "low"]);
    }

    #[test]
    fn ties_keep_declaration_order() {
        let t = tree(vec![("a", 5), ("b", 5), ("c", 5)], vec!["b", "a", "c"]);
        let merged: Vec<&str> = merge(&t, "default")
            .iter()
            .map(|d| d.name.as_str())
            .collect();
        assert_eq!(merged, vec!["b", "a", "c"]);
    }
}
