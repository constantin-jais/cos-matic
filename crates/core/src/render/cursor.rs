//! The `cursor` adapter: one `.cursor/rules/<domain>.mdc` per domain.
//!
//! Cursor rule files carry YAML frontmatter that controls *when* the rule
//! applies. This adapter therefore supports [`Feature::GlobActivation`]: a domain
//! with `globs` becomes a scoped rule (`alwaysApply: false`); a domain without
//! becomes an always-on rule. This is the multi-file, capability-bearing target
//! that exercises the gating model — adapters that lack this capability degrade
//! a domain's `globs` with a warning instead (see ADR: feature-gating-graceful-degradation).
//!
//! Domain names are validated upstream (`ir::build`) to be safe identifiers, so
//! they are used verbatim as filenames — no lossy slugging, hence no collisions.

use super::{
    Adapter, Feature, RenderInput, RenderOutput, RenderedFile, has_globs, require_output_dir,
};
use crate::error::Result;
use crate::ir::ResolvedDomain;

pub struct Cursor;

impl Adapter for Cursor {
    fn id(&self) -> &'static str {
        "cursor"
    }

    fn supports(&self, feature: Feature) -> bool {
        matches!(feature, Feature::GlobActivation)
    }

    fn render(&self, input: &RenderInput) -> Result<RenderOutput> {
        let dir = require_output_dir(input.target)?;
        let files = input
            .domains
            .iter()
            .map(|d| RenderedFile {
                path: format!("{dir}/{}.mdc", d.name),
                content: render_mdc(d),
            })
            .collect();
        Ok(RenderOutput {
            files,
            warnings: Vec::new(),
        })
    }
}

/// A `.cursor/rules/*.mdc` file: YAML frontmatter + Markdown body. Names and
/// globs are validated upstream, so they are YAML-safe here.
fn render_mdc(domain: &ResolvedDomain) -> String {
    let mut s = String::from("---\n");
    s.push_str(&format!("description: {}\n", domain.name));
    if has_globs(domain) {
        let globs = domain.globs.as_ref().expect("has_globs implies Some");
        s.push_str(&format!("globs: {}\n", globs.join(",")));
        s.push_str("alwaysApply: false\n");
    } else {
        s.push_str("alwaysApply: true\n");
    }
    s.push_str("---\n\n");
    s.push_str(domain.content.trim_end());
    s.push('\n');
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::schema::Target;

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
            name: "cursor".into(),
            adapter: "cursor".into(),
            output_file: None,
            output_dir: Some(".cursor/rules".into()),
            profile: "default".into(),
            subagents: vec![],
            skills: vec![],
            hooks: vec![],
        }
    }

    #[test]
    fn supports_glob_activation() {
        assert!(Cursor.supports(Feature::GlobActivation));
    }

    #[test]
    fn emits_one_mdc_file_per_domain_named_after_the_domain() {
        let a = domain("code-style", "Style", None);
        let b = domain("security", "Sec", None);
        let t = target();
        let input = RenderInput {
            domains: &[&a, &b],
            target: &t,
        };
        let out = Cursor.render(&input).unwrap();
        let paths: Vec<&str> = out.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(
            paths,
            vec![".cursor/rules/code-style.mdc", ".cursor/rules/security.mdc",]
        );
        assert!(out.warnings.is_empty());
    }

    #[test]
    fn globs_become_scoped_frontmatter() {
        let scoped = domain("rust", "Rust rules", Some(vec!["src/**/*.rs".into()]));
        let t = target();
        let input = RenderInput {
            domains: &[&scoped],
            target: &t,
        };
        let out = Cursor.render(&input).unwrap();
        let body = &out.files[0].content;
        assert!(body.contains("globs: src/**/*.rs"));
        assert!(body.contains("alwaysApply: false"));
        assert!(body.trim_end().ends_with("Rust rules"));
    }

    #[test]
    fn no_globs_or_empty_globs_means_always_apply() {
        let always = domain("general", "General", None);
        let empty = domain("empty-globs", "E", Some(vec![]));
        let t = target();
        let input = RenderInput {
            domains: &[&always, &empty],
            target: &t,
        };
        let out = Cursor.render(&input).unwrap();
        assert!(out.files[0].content.contains("alwaysApply: true"));
        // Empty globs are treated as "no globs", consistent across adapters.
        assert!(out.files[1].content.contains("alwaysApply: true"));
    }

    #[test]
    fn errors_without_output_dir() {
        let t = Target {
            output_dir: None,
            ..target()
        };
        let a = domain("a", "A", None);
        let input = RenderInput {
            domains: &[&a],
            target: &t,
        };
        assert!(Cursor.render(&input).is_err());
    }
}
