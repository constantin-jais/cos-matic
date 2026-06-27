//! The `claude` adapter: renders a single `CLAUDE.md` for Claude Code.
//!
//! In Phase 2 the body is the same priority-ordered concatenation as the
//! universal adapter — only the destination file differs (Tier 1.5: "same
//! content, different file"). Claude-only extensions (skills, subagents, hooks,
//! output-styles) are a later phase; like the universal adapter, it does not yet
//! honor glob activation and degrades it with a warning.

use super::{
    Adapter, Feature, RenderInput, RenderOutput, RenderedFile, concatenate, degradation_warnings,
    has_globs, require_output_file,
};
use crate::error::Result;

pub struct Claude;

impl Adapter for Claude {
    fn id(&self) -> &'static str {
        "claude"
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

    #[test]
    fn renders_claude_md_with_concatenated_content() {
        let a = ResolvedDomain {
            name: "a".into(),
            priority: 0,
            content: "Alpha".into(),
            globs: None,
        };
        let t = Target {
            name: "claude".into(),
            adapter: "claude".into(),
            output_file: Some("CLAUDE.md".into()),
            output_dir: None,
            profile: "default".into(),
        };
        let input = RenderInput {
            domains: &[&a],
            target: &t,
        };
        let out = Claude.render(&input).unwrap();
        assert_eq!(out.files[0].path, "CLAUDE.md");
        assert_eq!(out.files[0].content, "Alpha\n");
    }
}
