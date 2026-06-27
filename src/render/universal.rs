//! The `universal` adapter: renders [`AGENTS.md`](https://agents.md/), the
//! format read natively by many agents. Output is the domains' content in
//! priority order, separated by a blank line, normalized to a single trailing
//! newline so regeneration is byte-stable (important for drift detection).

use super::Adapter;
use crate::ir::ResolvedDomain;

pub struct Universal;

impl Adapter for Universal {
    fn id(&self) -> &'static str {
        "universal"
    }

    fn render(&self, domains: &[&ResolvedDomain]) -> String {
        let body = domains
            .iter()
            .map(|d| d.content.trim_end())
            .collect::<Vec<_>>()
            .join("\n\n");
        if body.is_empty() {
            String::new()
        } else {
            format!("{body}\n")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(name: &str, content: &str) -> ResolvedDomain {
        ResolvedDomain {
            name: name.to_string(),
            priority: 0,
            content: content.to_string(),
        }
    }

    #[test]
    fn concatenates_with_blank_line_and_single_trailing_newline() {
        let a = d("a", "Alpha\n\n");
        let b = d("b", "Beta");
        let out = Universal.render(&[&a, &b]);
        assert_eq!(out, "Alpha\n\nBeta\n");
    }

    #[test]
    fn empty_selection_renders_empty() {
        assert_eq!(Universal.render(&[]), "");
    }

    #[test]
    fn is_deterministic() {
        let a = d("a", "X");
        let b = d("b", "Y");
        assert_eq!(Universal.render(&[&a, &b]), Universal.render(&[&a, &b]));
    }
}
