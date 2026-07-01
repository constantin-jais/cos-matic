//! The `claude` adapter: `CLAUDE.md` (Tier 1.5) plus Claude-only Tier-2
//! constructs — subagents, skills, and hooks (ADR: claude-tier2-extensions). These are declared as
//! fields on the claude target, so only this adapter reads them; the neutral
//! domain/profile core is untouched. Names and frontmatter values are validated
//! because they become filenames and YAML keys.

use std::collections::BTreeMap;

use serde_json::{Value, json};

use super::{
    Adapter, Feature, RenderInput, RenderOutput, RenderedFile, concatenate, degradation_warnings,
    has_globs, require_output_file,
};
use crate::config::schema::{Hook, Skill, Subagent};
use crate::error::{Error, Result};

pub struct Claude;

impl Adapter for Claude {
    fn id(&self) -> &'static str {
        "claude"
    }

    fn supports(&self, feature: Feature) -> bool {
        matches!(
            feature,
            Feature::Subagents | Feature::Skills | Feature::Hooks
        )
    }

    fn render(&self, input: &RenderInput) -> Result<RenderOutput> {
        let mut files = vec![RenderedFile {
            path: require_output_file(input.target)?.to_string(),
            content: concatenate(input.domains),
        }];

        for sub in &input.target.subagents {
            check_name("subagent", &sub.name)?;
            check_single_line("subagent", &sub.name, &sub.description)?;
            files.push(RenderedFile {
                path: format!(".claude/agents/{}.md", sub.name),
                content: subagent_file(sub),
            });
        }

        for skill in &input.target.skills {
            check_name("skill", &skill.name)?;
            check_single_line("skill", &skill.name, &skill.description)?;
            files.push(RenderedFile {
                path: format!(".claude/skills/{}/SKILL.md", skill.name),
                content: skill_file(skill),
            });
        }

        if !input.target.hooks.is_empty() {
            files.push(RenderedFile {
                path: ".claude/settings.json".to_string(),
                content: settings_json(&input.target.hooks)?,
            });
        }

        Ok(RenderOutput {
            files,
            // claude still cannot honor glob activation; degrade it with a warning.
            warnings: degradation_warnings(
                self.id(),
                input.domains,
                Feature::GlobActivation,
                has_globs,
            ),
        })
    }
}

/// A name becomes a directory/file name and a YAML key, so it must be a safe
/// identifier.
fn check_name(kind: &str, name: &str) -> Result<()> {
    let mut chars = name.chars();
    let ok = matches!(chars.next(), Some(c) if c.is_ascii_alphanumeric())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if ok {
        Ok(())
    } else {
        Err(Error::InvalidExtension {
            kind: kind.to_string(),
            name: name.to_string(),
            reason: "name must match [A-Za-z0-9][A-Za-z0-9_-]*".to_string(),
        })
    }
}

/// A frontmatter scalar (e.g. `description`) must stay on one line.
fn check_single_line(kind: &str, name: &str, value: &str) -> Result<()> {
    if value.chars().any(|c| c.is_control()) {
        Err(Error::InvalidExtension {
            kind: kind.to_string(),
            name: name.to_string(),
            reason: "description must be a single line (no control characters)".to_string(),
        })
    } else {
        Ok(())
    }
}

fn subagent_file(sub: &Subagent) -> String {
    let mut s = String::from("---\n");
    s.push_str(&format!("name: {}\n", sub.name));
    s.push_str(&format!("description: {}\n", sub.description));
    if let Some(model) = &sub.model {
        s.push_str(&format!("model: {model}\n"));
    }
    if let Some(tools) = &sub.tools {
        s.push_str(&format!("tools: {}\n", tools.join(", ")));
    }
    s.push_str("---\n\n");
    s.push_str(sub.prompt.trim_end());
    s.push('\n');
    s
}

fn skill_file(skill: &Skill) -> String {
    let mut s = String::from("---\n");
    s.push_str(&format!("name: {}\n", skill.name));
    s.push_str(&format!("description: {}\n", skill.description));
    s.push_str("---\n\n");
    s.push_str(skill.content.trim_end());
    s.push('\n');
    s
}

/// Build a `.claude/settings.json` with a deterministic `hooks` block. Hooks are
/// grouped by event (a `BTreeMap` keeps the output sorted and stable).
fn settings_json(hooks: &[Hook]) -> Result<String> {
    let mut by_event: BTreeMap<&str, Vec<Value>> = BTreeMap::new();
    for hook in hooks {
        by_event
            .entry(hook.event.as_str())
            .or_default()
            .push(json!({
                "matcher": "",
                "hooks": [{ "type": "command", "command": hook.command }],
            }));
    }
    let settings = json!({ "hooks": by_event });
    serde_json::to_string_pretty(&settings).map_err(|e| Error::Serialize {
        what: "settings.json".to_string(),
        message: e.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::schema::Target;
    use crate::ir::ResolvedDomain;

    fn target() -> Target {
        Target {
            name: "claude".into(),
            adapter: "claude".into(),
            output_file: Some("CLAUDE.md".into()),
            output_dir: None,
            profile: "default".into(),
            subagents: vec![],
            skills: vec![],
            hooks: vec![],
        }
    }

    fn domain(name: &str, content: &str) -> ResolvedDomain {
        ResolvedDomain {
            name: name.into(),
            priority: 0,
            content: content.into(),
            globs: None,
        }
    }

    #[test]
    fn supports_claude_tier2_features() {
        assert!(Claude.supports(Feature::Subagents));
        assert!(Claude.supports(Feature::Skills));
        assert!(Claude.supports(Feature::Hooks));
        assert!(!Claude.supports(Feature::GlobActivation));
    }

    #[test]
    fn renders_claude_md_plus_tier2_files() {
        let mut t = target();
        t.subagents.push(Subagent {
            name: "reviewer".into(),
            description: "Reviews diffs.".into(),
            model: Some("sonnet".into()),
            tools: Some(vec!["Read".into(), "Grep".into()]),
            prompt: "You review code.".into(),
        });
        t.skills.push(Skill {
            name: "release".into(),
            description: "Cut a release.".into(),
            content: "Steps...".into(),
        });
        t.hooks.push(Hook {
            event: "PostToolUse".into(),
            command: "cargo fmt".into(),
        });
        let a = domain("a", "Alpha");
        let input = RenderInput {
            domains: &[&a],
            target: &t,
        };
        let out = Claude.render(&input).unwrap();
        let paths: Vec<&str> = out.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(
            paths,
            vec![
                "CLAUDE.md",
                ".claude/agents/reviewer.md",
                ".claude/skills/release/SKILL.md",
                ".claude/settings.json",
            ]
        );

        let agent = &out.files[1].content;
        assert!(agent.contains("name: reviewer"));
        assert!(agent.contains("model: sonnet"));
        assert!(agent.contains("tools: Read, Grep"));
        assert!(agent.trim_end().ends_with("You review code."));

        let settings = &out.files[3].content;
        assert!(settings.contains("\"PostToolUse\""));
        assert!(settings.contains("cargo fmt"));
    }

    #[test]
    fn no_tier2_means_just_claude_md() {
        let a = domain("a", "Alpha");
        let t = target();
        let input = RenderInput {
            domains: &[&a],
            target: &t,
        };
        let out = Claude.render(&input).unwrap();
        assert_eq!(out.files.len(), 1);
        assert_eq!(out.files[0].path, "CLAUDE.md");
    }

    #[test]
    fn rejects_unsafe_subagent_name() {
        let mut t = target();
        t.subagents.push(Subagent {
            name: "bad name".into(),
            description: "x".into(),
            model: None,
            tools: None,
            prompt: "p".into(),
        });
        let a = domain("a", "A");
        let input = RenderInput {
            domains: &[&a],
            target: &t,
        };
        let err = Claude.render(&input).unwrap_err();
        assert!(matches!(err, Error::InvalidExtension { .. }), "got {err:?}");
    }

    #[test]
    fn rejects_multiline_description() {
        let mut t = target();
        t.subagents.push(Subagent {
            name: "ok".into(),
            description: "line1\nmalicious: true".into(),
            model: None,
            tools: None,
            prompt: "p".into(),
        });
        let a = domain("a", "A");
        let input = RenderInput {
            domains: &[&a],
            target: &t,
        };
        let err = Claude.render(&input).unwrap_err();
        assert!(matches!(err, Error::InvalidExtension { .. }), "got {err:?}");
    }
}
