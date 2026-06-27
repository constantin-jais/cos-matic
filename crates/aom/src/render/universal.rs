//! The `universal` adapter: renders [`AGENTS.md`](https://agents.md/), read
//! natively by many agents (Claude Code, Cursor, Pi, Codex, …). One file; no
//! activation metadata, so a domain's `globs` degrade with a warning.

use super::{
    Adapter, Feature, RenderInput, RenderOutput, RenderedFile, concatenate, degradation_warnings,
    has_globs, require_output_file,
};
use crate::error::Result;

pub struct Universal;

impl Adapter for Universal {
    fn id(&self) -> &'static str {
        "universal"
    }

    fn supports(&self, _feature: Feature) -> bool {
        false
    }

    fn render(&self, input: &RenderInput) -> Result<RenderOutput> {
        let path = require_output_file(input.target)?;
        Ok(RenderOutput {
            files: vec![RenderedFile {
                path: path.to_string(),
                content: concatenate(input.domains),
            }],
            warnings: degradation_warnings(
                self.id(),
                input.domains,
                Feature::GlobActivation,
                has_globs,
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::schema::Target;
    use crate::ir::ResolvedDomain;

    fn domain(name: &str, content: &str, globs: Option<Vec<String>>) -> ResolvedDomain {
        ResolvedDomain {
            name: name.to_string(),
            priority: 0,
            content: content.to_string(),
            globs,
        }
    }

    fn target() -> Target {
        Target {
            name: "agents-md".into(),
            adapter: "universal".into(),
            output_file: Some("AGENTS.md".into()),
            output_dir: None,
            profile: "default".into(),
        }
    }

    #[test]
    fn renders_one_file_with_concatenated_content() {
        let a = domain("a", "Alpha", None);
        let b = domain("b", "Beta", None);
        let t = target();
        let input = RenderInput {
            domains: &[&a, &b],
            target: &t,
        };
        let out = Universal.render(&input).unwrap();
        assert_eq!(out.files.len(), 1);
        assert_eq!(out.files[0].path, "AGENTS.md");
        assert_eq!(out.files[0].content, "Alpha\n\nBeta\n");
        assert!(out.warnings.is_empty());
    }

    #[test]
    fn warns_and_still_renders_when_a_domain_uses_globs() {
        let scoped = domain("scoped", "Scoped", Some(vec!["src/**/*.rs".into()]));
        let t = target();
        let input = RenderInput {
            domains: &[&scoped],
            target: &t,
        };
        let out = Universal.render(&input).unwrap();
        assert_eq!(out.files[0].content, "Scoped\n");
        assert_eq!(out.warnings.len(), 1);
        assert!(out.warnings[0].contains("glob activation"));
    }

    #[test]
    fn empty_globs_do_not_warn() {
        // `Some(vec![])` is "no globs", consistent with the cursor adapter.
        let d = domain("d", "X", Some(vec![]));
        let t = target();
        let input = RenderInput {
            domains: &[&d],
            target: &t,
        };
        let out = Universal.render(&input).unwrap();
        assert!(out.warnings.is_empty());
    }

    #[test]
    fn errors_without_output_file() {
        let t = Target {
            output_file: None,
            ..target()
        };
        let a = domain("a", "A", None);
        let input = RenderInput {
            domains: &[&a],
            target: &t,
        };
        assert!(Universal.render(&input).is_err());
    }
}
