# Architecture Decision Records

Every non-obvious decision is recorded here, smallest unit per file, so the repo
doubles as teaching material. Format: context / decision / consequences.

| ADR                                                 | Decision                                            |
| --------------------------------------------------- | --------------------------------------------------- |
| [0001](0001-positioning-and-why-build.md)           | Why build this despite `ai-rulez`; learning wedge   |
| [0002](0002-language-rust.md)                       | Implementation language: Rust                       |
| [0003](0003-source-format-toml-plus-markdown.md)    | Source format: TOML manifest + referenced Markdown  |
| [0004](0004-safe-write-sentinel-lockfile.md)        | Safe-write sentinel: out-of-band lockfile           |
| [0005](0005-error-handling-miette.md)               | Error handling: `miette` diagnostics from the start |
| [0006](0006-adapter-output-model.md)                | Adapter output: a set of files, not one string      |
| [0007](0007-feature-gating-graceful-degradation.md) | Feature gating + graceful degradation               |
