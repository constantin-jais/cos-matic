# Contributing to cos-matic

This is a learning-first, clean-room project. The _reasoning_ is part of the
artifact: the goal is that anyone can read the repo and understand not just
_what_ it does, but _why_ every non-obvious choice was made.

## Start with the ADRs

Before proposing a change, read [`docs/adr/`](docs/adr/). Each record is one
decision, in `context / decision / consequences` form. The positioning and the
two distinctive subsystems live there:

- [ADR-0001](docs/adr/0001-positioning-and-why-build.md) — why this exists despite `ai-rulez`
- [ADR-0003](docs/adr/0003-source-format-toml-plus-markdown.md) — the source format
- [ADR-0004](docs/adr/0004-safe-write-sentinel-lockfile.md) — safe-write
- [ADR-0005](docs/adr/0005-error-handling-miette.md) — diagnostics-first errors

**Any architectural change ships with a new ADR.** No hidden design.

## The bar

- **Tests are the spec.** Non-trivial logic comes with tests; `cargo test` stays
  green.
- **Zero-warning lints.** `cargo clippy --all-targets --all-features` must pass;
  CI runs with `RUSTFLAGS="-D warnings"`.
- **Formatted.** `cargo fmt --all` before committing.
- **Deterministic by default.** No silent overwrites, no hidden state; the
  safe-write and drift guarantees must hold.
- **Commits in English**, descriptive, scoped.

A quick local pass before opening a PR:

```sh
cargo fmt --all --check
cargo clippy --all-targets --all-features
cargo test --all-features
```

## Reporting

Open an issue with a minimal reproduction (a `harness.toml` snippet and the
command you ran). For safe-write or drift behaviour, include the relevant
`.harness/lock.toml` state.

## License

By contributing, you agree your contributions are licensed under the project's
[MIT License](LICENSE).
