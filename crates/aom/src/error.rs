//! Crate-wide error type.
//!
//! Errors are `miette::Diagnostic`s (built on `thiserror`) so that, e.g., a TOML
//! parse failure renders with the offending span underlined and a `help` hint.
//! See ADR: error-handling-miette. No error message ever embeds a machine-local absolute path;
//! paths are repo-relative.

use miette::{Diagnostic, NamedSource, SourceSpan};
use thiserror::Error;

/// The single error type returned across the compiler pipeline.
#[derive(Debug, Error, Diagnostic)]
pub enum Error {
    /// A file could not be read from disk.
    #[error("could not read `{path}`")]
    #[diagnostic(code(aom::io))]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },

    /// The TOML manifest is syntactically invalid.
    ///
    /// Boxed: this variant carries the full source text for a pointed
    /// diagnostic, which would otherwise make every `Result<_, Error>` large
    /// (clippy::result_large_err).
    #[error(transparent)]
    #[diagnostic(transparent)]
    Parse(#[from] Box<ParseError>),

    /// A domain set neither or both of `content` / `content_file`.
    #[error("domain `{name}` must set exactly one of `content` or `content_file`")]
    #[diagnostic(
        code(aom::domain_content),
        help(
            "give the domain inline `content = \"...\"` or a `content_file = \"path.md\"`, not both"
        )
    )]
    DomainContent { name: String },

    /// A relative path escaped the project root (e.g. `../../etc`).
    #[error("path `{path}` escapes the project root")]
    #[diagnostic(
        code(aom::escaping_path),
        help("includes and content files must stay inside the project directory")
    )]
    EscapingPath { path: String },

    /// An absolute or machine-local path was used where a relative one is required.
    #[error("path `{path}` must be relative to the manifest, not absolute")]
    #[diagnostic(code(aom::absolute_path))]
    AbsolutePath { path: String },

    /// A domain's `content_file` does not exist.
    #[error("content file `{path}` for domain `{name}` was not found")]
    #[diagnostic(code(aom::missing_content_file))]
    MissingContentFile { name: String, path: String },

    /// A `library://` include or `builtins` entry names no known built-in.
    #[error("unknown built-in `{name}`")]
    #[diagnostic(
        code(aom::unknown_builtin),
        help("run `aom library list` to see the available built-ins")
    )]
    UnknownBuiltin { name: String },

    /// A goal references a check the tool does not implement.
    #[error("unknown goal check `{check}`")]
    #[diagnostic(
        code(aom::unknown_check),
        help("see the available checks in docs/adr/0009 (or `goals::CHECK_IDS`)")
    )]
    UnknownCheck { check: String },

    /// A goal is missing a parameter its check requires (e.g. a hard-gate
    /// `max-content-lines` with no `max`), which would make the gate a no-op.
    #[error("misconfigured goal `{check}`: {reason}")]
    #[diagnostic(code(aom::invalid_goal))]
    InvalidGoal { check: String, reason: String },

    /// One or more hard-gate goals failed.
    #[error("hard gate(s) failed:\n{}", .failures.join("\n"))]
    #[diagnostic(code(aom::goals_failed))]
    GoalsFailed { failures: Vec<String> },

    /// `[[includes]]` form a cycle.
    #[error("include cycle detected: {chain}")]
    #[diagnostic(code(aom::include_cycle))]
    IncludeCycle { chain: String },

    /// A profile selects a domain that does not exist.
    #[error("profile `{profile}` references unknown domain `{domain}`")]
    #[diagnostic(code(aom::unknown_domain))]
    UnknownDomain { profile: String, domain: String },

    /// A target selects a profile that does not exist.
    #[error("target `{target}` references unknown profile `{profile}`")]
    #[diagnostic(code(aom::unknown_profile))]
    UnknownProfile { target: String, profile: String },

    /// A target names an adapter the compiler does not implement.
    #[error("target `{target}` uses unknown adapter `{adapter}`")]
    #[diagnostic(
        code(aom::unknown_adapter),
        help("Phase 1 supports the `universal` adapter (AGENTS.md)")
    )]
    UnknownAdapter { target: String, adapter: String },

    /// A target did not declare where to write its output.
    #[error("target `{target}` is missing `output_file`")]
    #[diagnostic(code(aom::missing_output))]
    MissingOutput { target: String },

    /// Two domains / profiles / targets share a name, which is ambiguous.
    #[error("duplicate {kind} name `{name}`")]
    #[diagnostic(
        code(aom::duplicate_name),
        help("each domain, profile, and target name must be unique")
    )]
    DuplicateName { kind: String, name: String },

    /// Two targets write to the same output file.
    #[error("targets `{first}` and `{second}` both write to `{output_file}`")]
    #[diagnostic(code(aom::duplicate_output))]
    DuplicateOutput {
        output_file: String,
        first: String,
        second: String,
    },

    /// Serializing the lockfile or audit entry failed (should be infallible).
    #[error("failed to serialize {what}: {message}")]
    #[diagnostic(code(aom::serialize))]
    Serialize { what: String, message: String },

    /// A domain name is not a safe identifier (it flows into file paths and YAML
    /// frontmatter, so it must be constrained).
    #[error("invalid domain name `{name}`: {reason}")]
    #[diagnostic(
        code(aom::invalid_name),
        help(
            "domain names must match [A-Za-z0-9][A-Za-z0-9_-]* (used as filenames and YAML keys)"
        )
    )]
    InvalidName { name: String, reason: String },

    /// A glob pattern contains characters that would break YAML frontmatter.
    #[error("invalid glob `{glob}` in domain `{domain}`: {reason}")]
    #[diagnostic(code(aom::invalid_glob))]
    InvalidGlob {
        domain: String,
        glob: String,
        reason: String,
    },

    /// A target set both `output_file` and `output_dir`, which is ambiguous.
    #[error("target `{target}` sets both `output_file` and `output_dir`")]
    #[diagnostic(
        code(aom::ambiguous_output),
        help(
            "single-file adapters use `output_file`; multi-file adapters use `output_dir` — pick one"
        )
    )]
    AmbiguousOutput { target: String },

    /// Two rendered files resolved to the same output path within one run.
    #[error("two targets render to the same path `{path}`")]
    #[diagnostic(
        code(aom::duplicate_rendered_path),
        help("paths are compared case-insensitively; give the colliding targets distinct outputs")
    )]
    DuplicateRenderedPath { path: String },

    /// Safe-write refused to overwrite a hand-edited generated file.
    #[error("refusing to overwrite hand-edited file `{path}`")]
    #[diagnostic(
        code(aom::clobber),
        help("re-run with --force to overwrite, or delete the file to regenerate it")
    )]
    Clobber { path: String },

    /// `--check` found one or more generated files out of date with the source.
    #[error("drift detected: {} file(s) out of date:\n{}", .paths.len(), .paths.join("\n"))]
    #[diagnostic(code(aom::drift), help("run `aom generate` and commit the result"))]
    Drift { paths: Vec<String> },
}

/// A TOML parse failure with a pointed source span. Held behind a `Box` in
/// [`Error::Parse`] so the enum stays small.
#[derive(Debug, Error, Diagnostic)]
#[error("invalid TOML in `{name}`: {message}")]
#[diagnostic(code(aom::parse))]
pub struct ParseError {
    pub name: String,
    pub message: String,
    #[source_code]
    pub src: NamedSource<String>,
    #[label("here")]
    pub span: Option<SourceSpan>,
}

/// Convenience alias used throughout the crate.
pub type Result<T> = std::result::Result<T, Error>;
