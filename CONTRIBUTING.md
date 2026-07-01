# Contributing to bolt-cos-matic

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
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
bolt-cosmatic generate --check --manifest harness.toml
bolt-cosmatic goals --manifest harness.toml
```

If `bolt-cosmatic` is not installed, use `cargo run -q --bin bolt-cosmatic -- <command>`
for the two harness commands, then install it before treating it as routine.

## Reporting

Open an issue with a minimal reproduction (a `harness.toml` snippet and the
command you ran). For safe-write or drift behaviour, include the relevant
`.harness/lock.toml` state.

## Before opening a pull request

Please open an issue first for changes involving:

- architecture or public API changes;
- new dependencies;
- new product scope;
- security-sensitive behavior;
- storage, authentication, authorization, or provider changes;
- behavior that may affect determinism, reproducibility, privacy, or self-hosting.

Small documentation fixes, fixture additions, typo fixes, and focused tests can be opened directly as pull requests.

## Fixtures and examples

Fixtures and examples should be small, explicit, deterministic, safe to run locally, and free from secrets or personal data.

Prefer adding a new fixture over changing an existing one unless the existing behavior is wrong.

## Dependency policy

Avoid adding dependencies unless they are clearly justified.

New dependencies must be:

- permissive open source where possible: MIT, Apache-2.0, BSD, ISC, or MPL-2.0 preferred;
- compatible with self-hosting and local development;
- justified in the issue or pull request;
- accepted by the repository license and supply-chain checks when present;
- free from default telemetry, hidden network calls, and unnecessary SaaS coupling.

Discuss before adding:

- LGPL, GPL, or other copyleft dependencies;
- AGPL dependencies;
- source-available or non-OSI licenses such as SSPL or BSL;
- opaque SDKs;
- dependencies that introduce external providers, storage, auth, analytics, telemetry, or hosted services.

Avoid unnecessary vendor lock-in, proprietary services by default, telemetry by default, and dependencies that make self-hosting harder.

When a dependency is required, explain why a small local implementation is not enough.

## Good first contributions

Good first contributions include improving docs, adding examples, adding fixture cases, improving error messages, adding tests around existing behavior, and making quickstart instructions easier to follow.

## License

By contributing, you agree your contributions are licensed under the project's
[MIT License](LICENSE).
