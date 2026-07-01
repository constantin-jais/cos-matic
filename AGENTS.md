# bolt-cos-matic Session Doctrine

- Treat `bolt-cosmatic` as the local source of truth for agent configuration, gates,
  and repeatable session bootstrap.
- Start work from observed repository state: git status, generated-config drift,
  declared goals, then targeted tests.
- Keep autonomous actions bounded: dry-run is routine; live loop is sandbox-only
  and requires scoped service credentials.
- Prefer deterministic gates over memory. If an instruction matters across
  sessions, put it in `harness.toml`, a domain file, an ADR, or a test.

# Stack Authority

Rust is the default implementation language for every durable system capability.

Durable means the code decides, stores, authorizes, orchestrates, builds,
deploys, verifies, or survives beyond a UI session.

## Ownership rules

- **Rust owns durable logic**: core libraries, services, CLIs, workers,
  orchestration, auth, persistence access, policy evaluation, inspection,
  release tooling, and portability gates.
- **TypeScript owns browser-facing experience**: UI, client API glue, form
  validation, Playwright tests, and thin browser-adjacent integration code.
- **Bun is a web-app convenience**, not a platform foundation.
- **Zig owns nothing architecturally.** It may appear only as a hidden helper
  behind Rust-owned tooling, for example through `cargo-zigbuild`.
- **Shell is temporary glue.** Durable automation must migrate to Rust.

## Exception rule

Any exception requires an ADR with:

1. why Rust is not appropriate;
2. expected lifetime of the exception;
3. migration path back to Rust or deletion;
4. security, quality, performance, portability, and sovereignty impact.

If the code is durable and there is no ADR, implement it in Rust.

# Rust Core

Rust is the core language for this ecosystem because it gives deterministic
binaries, memory safety without a garbage collector, strong typing, excellent
CLI/service ergonomics, and a credible path to portable artifacts.

## Standard choices

- Async runtime: `tokio`.
- Web services: `axum` + `tower`, with graceful shutdown.
- HTTP client: `reqwest` with `default-features = false` and `rustls` features.
- Serialization: `serde` at boundaries only; domain models stay explicit.
- Database: `sqlx` with PostgreSQL; prefer compile-time checked queries.
- Auth: Biscuit tokens with local validation and typed extractors.
- Observability: `tracing`, structured JSON in production, OpenTelemetry when
  distributed traces are needed.
- Object storage: `object_store` over provider-specific SDKs.
- Jobs: `tokio` for non-critical local work; persisted queues for critical work.
- Errors: `thiserror` for libraries, `miette`/`anyhow` for binaries where
  diagnostics matter.

## Domain discipline

- Parse external input into valid domain types at the boundary.
- Use newtypes for identifiers, tenant IDs, email addresses, percentages,
  non-empty names, and other constrained values.
- Keep DTOs separate from domain types.
- Prefer `TryFrom`/constructors that enforce invariants over repeated runtime
  validation.
- `unwrap()` is forbidden in production paths unless the invariant is local,
  documented, and mechanically obvious.

## Portability discipline

- The portable core is pure Rust and dependency-light.
- Native-only dependencies are forbidden in core crates unless an ADR accepts the
  portability cost.
- Prefer Rust-native TLS, crypto, compression, and storage crates.
- No OpenSSL or `native-tls` in portable paths.
- Cross-target builds are release artifacts, not afterthoughts. Each artifact
  records target triple, version, checksum, provenance, and build inputs.

## Release profile

Use explicit release settings for binaries that are shipped:

```toml
[profile.release]
lto = true
codegen-units = 1
strip = true
panic = "abort"
```

Deviations require measurement or a documented operational reason.

## CI gates

- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test` or `cargo nextest run`
- `cargo deny check`
- `cargo audit` or an equivalent RustSec gate with documented exceptions
- portable-core compile check when a crate is declared portable, for example
  `wasm32-unknown-unknown` or the supported release target matrix.

## Forbidden without ADR

- Backend or durable business logic implemented in TypeScript.
- Durable automation implemented as shell scripts.
- Direct dependency on OpenSSL/native TLS.
- Provider-specific storage SDKs when a neutral Rust abstraction works.
- Reimplementing Rust core logic in another language instead of binding it.

# Sovereign Stack

Sovereignty is a system property, not a branding claim.

## Default stance

- Prefer open-source components with auditable code and standard protocols.
- Avoid US hyperscalers and SaaS dependencies for core truth, runtime, storage,
  auth, observability, and AI workflows.
- Prefer EU providers with EU data residency and contractual clarity.
- Keep self-hostable or provider-pluggable paths for critical capabilities.
- Document every non-sovereign dependency as an explicit risk acceptance.

## Sovereignty criteria

A component is sovereign only when these properties are satisfied or explicitly
accounted for:

1. data residency is in the EU;
2. the operator is not exposed to CLOUD Act-like foreign control for the data;
3. the component is open-source or has an audited replacement path;
4. deployment and export paths do not lock core truth into one provider.

## License floor

- Preferred: MIT, Apache-2.0, BSD-2/3, ISC.
- Acceptable with review: MPL-2.0.
- Requires legal/architecture validation: LGPL, GPL.
- Forbidden by default: AGPL for network services, SSPL, BSL/source-available
  traps, proprietary SDKs in core paths.

## Provider posture

- Clever Cloud is the default sovereign PaaS target.
- Clever AI or EU/self-hostable model providers are preferred for runtime AI.
- Provider-specific APIs stay behind interfaces and adapters.
- No dependency may silently make AWS, GCP, Azure, Vercel, Netlify, Auth0,
  Datadog, Algolia, Firebase, or equivalent SaaS part of the core system.

## Gateable checks

- dependency license audit;
- registry/source allowlist;
- no hardcoded US SaaS endpoints in core paths;
- documented DPA/data-residency for external services;
- export path for data and artifacts;
- no secrets or PII in logs, traces, prompts, or release metadata.

# Artifact-First Release

A release ships immutable artifacts, not hopes that production rebuilds the same
thing later.

## Doctrine

- Build once per supported target in CI or a controlled builder.
- Attach evidence before exposure: tests, audits, checksums, SBOM, provenance,
  and release manifest.
- Promote pointers to already-built artifacts; do not rebuild during promotion.
- Deployment rollback means repointing to a previous known-good artifact.
- Distribution compensation means yank/deprecate/replace forward; never pretend
  irreversible registries can roll back.

## Required artifact metadata

Each release artifact records:

- package name and version;
- git commit and dirty-state flag;
- target triple or platform identifier;
- build profile and relevant feature flags;
- SHA-256 checksum;
- SBOM location or embedded SBOM marker;
- signature or attestation reference;
- smoke-test evidence;
- rollback or compensation path.

## Build matrix

Rust owns the build matrix. Cross-compilation helpers are implementation details,
not architectural owners. A target is supported only when its build, install,
smoke test, and sovereign floor are documented.

## Gates

A release must fail if:

- an artifact has no checksum;
- provenance is missing;
- license or advisory gates are red without documented exception;
- the deploy path rebuilds instead of deploying a recorded artifact;
- a supported platform has only a store channel and no store-free install path;
- secrets are required where OIDC/keyless publishing is available.

# Security Baseline

- Validate and sanitize every external input at the trust boundary; never trust
  client-supplied data.
- Never log secrets, credentials, tokens, or personally identifiable information.
  Redact before logging.
- Keep secrets out of source control and out of error messages; load them from
  the environment or a secrets manager at runtime.
- Prefer least privilege: the narrowest scope, the shortest-lived token, the
  smallest blast radius.
- Treat dependencies as attack surface: pin versions, review licenses, and audit
  for known vulnerabilities.

# Biscuit Auth

Use Biscuit tokens for attenuable, locally-verifiable authorization. Treat them
as logic programs, not opaque JWT replacements.

## Token model

- Authority block is created only by the auth issuer.
- Tokens carry facts and checks; services provide authorizer facts and policies.
- Closed world: what is not explicitly allowed is denied.
- Attenuation may restrict rights but never expand them.
- Tokens must expire through a token check, not only through service-side logic.

## Required authority facts

```datalog
user("user_id");
tenant("tenant_id");
role("user_id", "role");
check if time($time), $time < 2026-12-31T23:59:59Z;
```

- `tenant()` is mandatory for multi-tenant isolation.
- Do not store PII, passwords, secrets, or emails in tokens.
- Prefer short TTLs and explicit attenuation for delegated operations.

## Authorizer rules

- Inject service context: `time`, `resource`, `operation`, tenant boundary, and
  any request-specific facts.
- Keep policies in the authorizer and checks in the token.
- End with an explicit deny policy.
- Test every policy with allow and deny fixtures.

## Rust integration

- Validate locally using the public key set.
- Expose a typed extractor/middleware that returns a validated principal.
- Redact tokens in logs and tracing spans.
- Cache revocation checks only with short TTLs.
- Rotate keys by accepting old and new public keys until old tokens expire.

## Defense in depth

- Pair token tenant facts with PostgreSQL RLS where persistence is multi-tenant.
- Include authorization scenarios in contract/integration tests.
- Version `.datalog` policy fixtures and include them in release evidence.

## Forbidden

- JWT as the default internal authorization format.
- Client-side token creation.
- Tokens without expiration.
- Tokens without tenant facts for tenant-scoped systems.
- Shared private keys across services.
- Logging full token contents.

# Decision Axes

When a choice is non-obvious, decide on these four axes, in priority order. When
they conflict, the earlier axis wins.

1. **Security** — attack surface, threat model, no secrets or PII in logs,
   validate untrusted input at the boundary.
2. **Quality** — readability over cleverness, strict types, no panics on input,
   no dead code, lints with zero warnings.
3. **Performance** — measure hot paths, avoid needless allocation, prefer async
   for I/O, watch for N+1.
4. **Completeness** — done means code plus tests plus docs, within the stated
   scope.

These are the only valid criteria. Effort, calendar estimates, and "MVP vs
nice-to-have" are not decision axes here — reason in terms of technical
complexity. Flag over-engineering (a solution larger than its scope) as its own
defect, distinct from incompleteness.

# Web Boundary

TypeScript is mandatory for frontend source code. JavaScript source files are
forbidden.

Bun is the preferred web toolchain, not an architectural boundary and not a core
runtime. It is acceptable only while it reduces dependency surface and remains
replaceable without changing product contracts.

## TypeScript owns

- Browser-facing UI and UX workflows.
- Presentation state.
- Form validation at the UI boundary.
- Browser APIs.
- Generated clients and generated types for Rust-owned APIs.
- Web tests and E2E flows.
- Design/prototyping surfaces.

## TypeScript does not own

- Durable business logic.
- API contracts written by hand.
- Authorization or policy truth.
- Persistence or database migrations.
- Background jobs and schedulers.
- Agent orchestration.
- LLM runtime decisions.
- Release, signing, provenance, or deployment tooling.

## Bun posture

Bun may own dependency installation, web app scripts, browser bundle builds,
local dev commands, and frontend tests where it is sufficient.

Bun APIs must not become architecture by accident. `Bun.serve` is allowed for
local dev servers, SSR/web-app-local serving, and disposable mocks. Any durable
or externally-consumed TypeScript server boundary requires an ADR explaining why
Rust is not the right seam.

## Framework posture

Use browser/framework primitives for UI. Do not add Hono, Elysia, Express,
Fastify, or another TypeScript backend framework unless an ADR justifies the
server boundary. The issue is not the framework name; it is TypeScript owning a
durable backend capability.

## Generated contracts

Frontend TypeScript consumes Rust-owned contracts. Prefer generated clients and
types from OpenAPI, WIT, or another Rust-owned schema. Do not hand-write a
TypeScript type for a Rust-owned API contract when generation is possible.

## Source rules

- `.ts` and `.tsx` are the frontend source formats.
- `.js`, `.jsx`, `.mjs`, and `.cjs` source files are forbidden unless generated,
  unversioned, or explicitly allowed by policy.
- `strict: true` is mandatory in `tsconfig.json`.
- `allowJs: true` is forbidden.
- Do not create TypeScript server code before challenging whether it belongs in
  Rust.

# Code Style

- Explicit is better than implicit; readability beats concision.
- Prefer duplication over the wrong abstraction (rule of three: extract a shared
  abstraction only on the third occurrence).
- Comments explain the _why_, not the _what_. Annotate deliberate debt with a
  reason.
- Match the surrounding code: its naming, its idioms, its comment density. New
  code should read like the code already there.
- Keep units small and single-purpose. When a file grows large, that is a signal
  it is doing too much.

# Test-Driven Development

- Write a test for any non-trivial logic, alongside (or before) the
  implementation. Tests are the executable specification.
- Cover the real edge cases: empty input, malformed input, boundary values, and
  each distinct error path — assert the specific error, not just "it failed".
- A change is not done until its tests are green. Never claim success without
  running the verification and confirming the output.
- When fixing a bug, first add a test that reproduces it (red), then fix it
  (green). The test prevents the regression from returning.

# Testing Strategy

Tests prove behavior and protect decisions. They are not a ceremony around the
implementation.

## Principles

- Every non-trivial logic change ships with tests.
- Every bug fix starts with a regression test when reproduction is possible.
- Test behavior, not private implementation details.
- A flaky test is a production incident in the test suite: fix it or remove it.
- Prefer real boundaries for critical workflows: database, filesystem, HTTP,
  auth, and generated artifacts.

## Rust-first test stack

- Unit tests live beside the code they specify.
- Integration tests live in `tests/` when multiple components are assembled.
- Prefer `cargo nextest` for fast CI execution when available.
- Use `rstest` for parameterized cases.
- Use `proptest` for combinatorial or parser/serializer behavior.
- Use `wiremock` for external HTTP boundaries.
- Use `testcontainers` or ephemeral schemas for PostgreSQL integration.
- Use `insta` only for stable, reviewed textual artifacts.
- Use `cargo llvm-cov` or equivalent for coverage evidence.

## Contract and artifact tests

- Generated configs should be snapshot-tested only when the snapshot is the
  product contract.
- API schemas must be generated from source and diffed for breaking changes.
- Release artifacts need smoke tests from the artifact itself, not from the
  workspace that produced it.
- Policy files need golden allow/deny fixtures.

## CI shape

- Fast quality gates first: format, lint, manifest validation.
- Unit and focused integration tests on every push.
- Heavier E2E and release smoke tests before merge/release.
- Coverage is a regression signal, not a vanity metric; critical modules need
  stronger thresholds than peripheral adapters.

## Test naming

Name tests as specifications: `rejects_expired_token`,
`renders_cursor_rules_with_frontmatter`, `refuses_drift_without_force`.

# Agent Behavior

- **Read real state before acting.** Before continuing prior work or fixing a
  failure, check the ground truth — version control status, the test suite, the
  actual diff — rather than trusting a summary. If the state is already green or
  the work is already done, say so before acting.
- **Verify before prescribing.** Do not recommend a config option, flag, or API
  as fact unless you have confirmed it exists in the target version. If unsure,
  present it as a hypothesis, not a prescription.
- **Never write machine-local absolute paths** into versioned files (no
  `/Users/<name>/...`). Use repo-relative paths, `$HOME`/`~`, or resolve from the
  project root.
- **Do not clobber uncommitted work.** Distinguish files you changed this session
  from pre-existing local changes; confirm before committing the latter.

# Anti Gold-Plating

Build what the task needs, not what it might one day need.

- Flag when a solution is over-engineered relative to its scope. An abstraction
  with one implementation, a config knob nobody sets, a generic layer for a single
  case — these are defects, not foresight.
- Distinguish _scope_ (what was asked) from _completeness_ (doing the asked thing
  fully, with tests and docs). Cut speculative generality; keep real completeness.
- Add the abstraction when the second or third concrete case arrives, not before.
- If you bound coverage (sampling, a top-N, skipping a path), say so explicitly —
  silent truncation reads as "covered everything".

# Factual Style

State a proposition and its justification directly. Do not preface with sincerity
markers ("honestly", "to be frank") or hedge with decorative qualifiers.

- Replace vague confidence with calibrated uncertainty: say "unverified",
  "untested hypothesis", or "low confidence" when that is the real state.
- When a choice is non-obvious, add one or two lines on the _why_, not the _what_.
- Prefer an uncomfortable truth over hollow encouragement. Challenge weak
  decisions; surface trade-offs instead of agreeing reflexively.
- No filler, no padding, no unsolicited follow-up offers at the end of a turn.

# Response Blocks

Structure status and proposals with these named blocks. Use a block only when its
trigger applies; omit any slot that has no content (an empty slot is itself
information — never pad with "N/A").

- **ORIENTATION** — when starting or resuming work. Slots: Subject, Depends on,
  Blocked by, In parallel, Protocol, Risk.
- **PROPOSITION** — before a non-trivial change (more than ~3 files, cross-module,
  or architectural). Slots: Context, Preserve, Modules, Interfaces, Steps, Tests,
  Non-goals, Acceptance, Refactor risk.
- **DELTA** — after a unit of work. Slots: Done (+ evidence), Remaining, Blocked.
  Cite proof for "Done"; evidence beats assertion.
- **DECISION** — at a fork affecting an axis, or genuinely ambiguous scope. Slots:
  Options (2-3 with trade-offs), Recommendation, Reversibility.
