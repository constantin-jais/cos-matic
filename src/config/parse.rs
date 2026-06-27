//! Parse a `harness.toml` string/file into a [`Manifest`], mapping TOML errors
//! to a pointed `miette` diagnostic (ADR-0005).

use std::path::Path;

use miette::{NamedSource, SourceSpan};

use crate::config::schema::Manifest;
use crate::error::{Error, ParseError, Result};

/// Parse manifest text. `name` is a display label (e.g. the repo-relative path)
/// used in diagnostics — never a machine-local absolute path.
pub fn parse_str(name: &str, text: &str) -> Result<Manifest> {
    toml::from_str::<Manifest>(text).map_err(|e| {
        let span = e
            .span()
            .map(|r| SourceSpan::from((r.start, r.end - r.start)));
        Error::Parse(Box::new(ParseError {
            name: name.to_string(),
            message: e.message().to_string(),
            src: NamedSource::new(name, text.to_string()),
            span,
        }))
    })
}

/// Read and parse a manifest file. `display_name` is the repo-relative label.
pub fn parse_file(path: &Path, display_name: &str) -> Result<Manifest> {
    let text = std::fs::read_to_string(path).map_err(|source| Error::Io {
        path: display_name.to_string(),
        source,
    })?;
    parse_str(display_name, &text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_minimal_manifest() {
        let text = r#"
            [package]
            name = "demo"

            [[domains]]
            name = "code-style"
            priority = 8
            content = "be explicit"

            [[profiles]]
            name = "default"
            domains = ["code-style"]

            [[targets]]
            name = "agents-md"
            adapter = "universal"
            output_file = "AGENTS.md"
            profile = "default"
        "#;
        let m = parse_str("harness.toml", text).expect("should parse");
        assert_eq!(m.package.name, "demo");
        assert_eq!(m.domains.len(), 1);
        assert_eq!(m.domains[0].name, "code-style");
        assert_eq!(m.domains[0].priority, 8);
        assert_eq!(m.profiles[0].domains, vec!["code-style".to_string()]);
        assert_eq!(m.targets[0].adapter, "universal");
    }

    #[test]
    fn ignores_unknown_and_future_fields() {
        // Forward-compatibility: not-yet-implemented sections must not break parsing.
        let text = r#"
            [package]
            name = "demo"
            description = "a field we do not model yet"

            [[domains]]
            name = "d"
            content = "x"

            [[goals]]
            kind = "hard_gate"
            name = "no-secrets"

            [[targets]]
            name = "claude"
            adapter = "claude"
            profile = "default"
            subagents = [{ name = "reviewer", model = "opus" }]
            hooks = [{ event = "sessionStart" }]
        "#;
        let m = parse_str("harness.toml", text).expect("future fields are ignored");
        assert_eq!(m.package.name, "demo");
        assert_eq!(m.domains.len(), 1);
        assert_eq!(m.targets.len(), 1);
    }

    #[test]
    fn reports_a_pointed_error_on_bad_toml() {
        let text = "[package]\nname = \n";
        let err = parse_str("harness.toml", text).unwrap_err();
        match err {
            Error::Parse(e) => assert!(e.span.is_some(), "should carry a span"),
            other => panic!("expected Parse error, got {other:?}"),
        }
    }
}
