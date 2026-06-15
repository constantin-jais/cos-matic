# Link Cable — Extraction Plan from cos-matic

> **For agentic build workers:** this is an implementation plan. Execute task-by-task, keep checkboxes updated, and do not broaden scope without a new ADR/plan. The target repository is `https://github.com/constantin-jais/gear-cable`.

**Goal:** extract the multi-platform distribution product from cos-matic into a dedicated Rust-first repository named **Link Cable**, without breaking cos-matic and without inventing platform UI/application logic prematurely.

**Progress (2026-06-29):** Link Cable is initialized and pushed at `constantin-jais/gear-cable` (`7b2ecb8`) with green CI. Tasks 0, 1, 2, 6, and 7 are complete; Task 3 policy gates are partially complete; local build/checksum and publish flows remain open.

**Product boundary:** Link Cable owns the distribution substrate: build matrix, artifact model, install/update/doctor flows, release manifests, signatures/checksums/provenance, bindings around one Rust core, and channel-specific publishing primitives. cos-matic remains the agent/autonomy product and becomes the first consumer.

**Source doctrine in cos-matic:**

- `docs/adr/0029-portability-rust-core-bind-not-reimplement.md` — one Rust core, generated bindings, no reimplementation.
- `docs/adr/0030-distribution-doctrine-append-only.md` — forward-only publish, `compensate` not rollback.
- `docs/adr/0031-supply-chain-and-sovereignty.md` — keyless by default, provenance, SBOM, sovereign floor.
- `docs/adr/0032-native-ui-and-binding-matrix.md` — native UI per platform over one Rust core.
- `README.md` lines around the architecture table — current public positioning mentions the future distribution seam.

## Non-negotiable constraints

- **Security first:** no long-lived registry token unless a channel makes it unavoidable; prefer OIDC/keyless publishing.
- **No secrets in git, logs, plan files, release manifests, fixtures, or audit logs.**
- **Append-only distribution semantics:** never call publish recovery a rollback; use `compensate`.
- **Sovereign floor:** every supported platform must have at least one store-free install path, except iOS where the EU DMA caveat must be explicit.
- **Single Rust core:** no Swift/Kotlin/TypeScript reimplementation of distribution logic.
- **Generated bindings only:** hand-written native code may be UI or thin host glue, not business logic.
- **cos-matic stays green:** extraction must not regress `cargo fmt`, `cargo clippy`, `cargo test`, dependency audit, or dogfood checks.
- **Small reversible steps:** first migrate doctrine and scaffolding, then shared crates, then consumers.

## Target architecture

```text
gear-cable/
  Cargo.toml
  rust-toolchain.toml
  deny.toml
  README.md
  LICENSE
  docs/
    adr/
    distribution-doctrine.md
    platform-matrix.md
    release-runbook.md
  crates/
    gear-cable-core/       # pure Rust core: manifests, targets, plans, artifact graph
    gear-cable-cli/        # `gear-cable` binary: plan/build/smoke/promote/compensate/doctor
    gear-cable-dist/       # channel traits + implementations, no app-specific logic
    gear-cable-bindings/   # feature-gated generated bindings host crate, if needed
  schemas/
    gear-cable.manifest.schema.json
  examples/
    cos-matic/
      gear-cable.toml
  scripts/
    audit-deps.sh
  .github/workflows/
    ci.yml
    release.yml            # starts dry-run only until credentials/policies are explicit
```

### Crate responsibilities

#### `gear-cable-core`

Pure, deterministic, I/O-light Rust library.

Owns:

- platform model: OS, arch, libc/ABI, package kind;
- artifact model: name, version, target triple, checksum, signature, provenance;
- release manifest parser/validator;
- distribution plan computation;
- sovereign-floor policy validation;
- serializable diagnostics suitable for bindings.

Must not own:

- GitHub-specific orchestration;
- cos-matic manifest compilation;
- app UI;
- network publishing side effects.

Initial public API sketch:

```rust
pub struct ReleaseManifest { /* parsed from gear-cable.toml */ }
pub struct DistributionPlan { /* artifacts + channel actions */ }
pub struct Artifact { /* path, target, kind, checksum, provenance */ }
pub struct PolicyReport { /* gates + warnings */ }

pub fn parse_manifest(input: &str) -> Result<ReleaseManifest>;
pub fn plan(manifest: &ReleaseManifest, host: Host) -> Result<DistributionPlan>;
pub fn validate_policy(manifest: &ReleaseManifest) -> Result<PolicyReport>;
```

#### `gear-cable-dist`

Side-effect boundary for distribution channels.

Owns the doctrine from ADR-0030:

```rust
#[async_trait]
pub trait Distributor {
    async fn plan(&self, req: PlanRequest) -> Result<PlanReport>;
    async fn publish_prerelease(&self, req: PublishRequest) -> Result<PublishReport>;
    async fn smoke(&self, req: SmokeRequest) -> Result<SmokeReport>;
    async fn promote(&self, req: PromoteRequest) -> Result<PromoteReport>;
    async fn compensate(&self, req: CompensateRequest) -> Result<CompensateReport>;
}
```

Early implementations should be minimal:

- `direct-download` / GitHub Releases draft or equivalent artifact folder;
- `crate` only if/when publishing Link Cable itself;
- `oci` later;
- app stores deferred until policy and signing isolation are designed.

#### `gear-cable-cli`

Thin command surface over `core` and `dist`:

```sh
gear-cable doctor
gear-cable plan --manifest gear-cable.toml
gear-cable build --manifest gear-cable.toml
gear-cable smoke --manifest gear-cable.toml --channel direct
gear-cable publish-prerelease --manifest gear-cable.toml --channel direct
gear-cable promote --manifest gear-cable.toml --channel direct
gear-cable compensate --manifest gear-cable.toml --channel direct --version X.Y.Z
```

All mutating commands need `--yes` or CI policy approval. Default local mode is read-only/dry-run.

## Initial `gear-cable.toml` shape

```toml
[package]
name = "cos-matic"
version = "0.0.0"
repository = "https://github.com/constantin-jais/cos-matic"

[core]
language = "rust"
workspace = "."
binary = "aom"

[policy]
append_only = true
keyless_preferred = true
require_slsa = true
require_sbom = true
require_checksums = true
require_signatures = true
require_sovereign_floor = true

[[platforms]]
os = "linux"
arch = "x86_64"
target = "x86_64-unknown-linux-gnu"
packages = ["tar.gz", "appimage"]
sovereign_floor = ["direct-download"]

[[platforms]]
os = "linux"
arch = "aarch64"
target = "aarch64-unknown-linux-gnu"
packages = ["tar.gz", "appimage"]
sovereign_floor = ["direct-download"]

[[platforms]]
os = "macos"
arch = "aarch64"
target = "aarch64-apple-darwin"
packages = ["tar.gz", "dmg"]
sovereign_floor = ["direct-download"]

[[platforms]]
os = "windows"
arch = "x86_64"
target = "x86_64-pc-windows-msvc"
packages = ["zip", "installer"]
sovereign_floor = ["direct-download"]

[[channels]]
name = "direct"
kind = "direct-download"
prerelease = true
stable = true
```

Do not overfit this schema in the first commit. The first implementation may support only `linux x86_64` + `macos aarch64` tarballs, as long as unsupported platforms fail explicitly.

## Task 0 — Prepare Link Cable repository

**Files in `gear-cable`:** create base project files.

- [x] Clone/open the new repo:

```bash
git clone https://github.com/constantin-jais/gear-cable.git
cd gear-cable
```

- [x] Add `LICENSE` matching intended license, expected: MIT unless explicitly changed.
- [x] Add `README.md` with this positioning:
  - “Rust-first distribution substrate for multi-platform developer tools.”
  - “Forward-only releases, signed artifacts, sovereign install floors.”
  - “cos-matic is the first consumer.”
- [x] Add `rust-toolchain.toml` pinned to stable or the same minimum used by cos-matic if required.
- [x] Add root `Cargo.toml` workspace:

```toml
[workspace]
resolver = "3"
members = [
  "crates/gear-cable-core",
  "crates/gear-cable-cli",
  "crates/gear-cable-dist",
]

[workspace.package]
edition = "2024"
rust-version = "1.95"
license = "MIT"
repository = "https://github.com/constantin-jais/gear-cable"

[workspace.dependencies]
clap = { version = "4", features = ["derive"] }
miette = { version = "7", features = ["fancy"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
toml = "0.8"
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
async-trait = "0.1"
tempfile = "3"
gear-cable-core = { path = "crates/gear-cable-core" }
gear-cable-dist = { path = "crates/gear-cable-dist" }

[profile.release]
strip = true
lto = "thin"
```

- [x] Add `.gitignore` for `target/`, local env files, generated artifacts.
- [x] Add `deny.toml` and `scripts/audit-deps.sh` based on cos-matic, but remove obsolete exceptions unless the same transitive advisories appear.

**Acceptance:** `cargo metadata` succeeds.

## Task 1 — Port the doctrine, not the old code

**Files in `gear-cable`:**

- `docs/adr/0001-product-boundary.md`
- `docs/adr/0002-portability-rust-core-bindings.md`
- `docs/adr/0003-forward-only-distribution.md`
- `docs/adr/0004-supply-chain-sovereignty.md`
- `docs/adr/0005-native-bindings-matrix.md`
- `docs/adr/README.md`

**Instructions:**

- [x] Copy the substance of cos-matic ADR-0029 through ADR-0032.
- [x] Rewrite names from “cos-matic” to “Link Cable” where ownership moved.
- [x] Keep cos-matic references only as “first consumer / origin doctrine”.
- [x] Preserve the key decisions:
  - Rust core, bind don’t reimplement;
  - generated bindings;
  - append-only, forward-only, compensate-not-rollback;
  - keyless/OIDC by default;
  - SLSA/SBOM/checksums/signatures;
  - sovereign floor.
- [x] Mark deferred items explicitly: app stores, notarization, mobile UI, Windows C# binding.

**Acceptance:** docs explain why Link Cable exists without implying cos-matic now owns distribution.

## Task 2 — Scaffold crates and minimal APIs

**Files:**

```text
crates/gear-cable-core/Cargo.toml
crates/gear-cable-core/src/lib.rs
crates/gear-cable-core/src/error.rs
crates/gear-cable-core/src/manifest.rs
crates/gear-cable-core/src/platform.rs
crates/gear-cable-core/src/plan.rs
crates/gear-cable-core/src/policy.rs
crates/gear-cable-dist/Cargo.toml
crates/gear-cable-dist/src/lib.rs
crates/gear-cable-cli/Cargo.toml
crates/gear-cable-cli/src/main.rs
crates/gear-cable-cli/src/cli.rs
```

**Implementation constraints:**

- [x] Use `serde` structs for manifest parsing.
- [x] Use `miette` for CLI diagnostics; use serializable DTOs in core errors where possible.
- [x] No network clients yet.
- [x] No signing implementation yet; model the requirement and fail clearly when missing.
- [x] Unit tests for parsing and policy validation before CLI integration.

**Acceptance commands:**

```bash
cargo fmt --all --check
cargo test --workspace
cargo clippy --workspace --all-targets --all-features
```

## Task 3 — Implement policy gates first

**Core gates:**

- [x] `append_only` must default to true.
- [x] `require_checksums` true means every planned artifact must have a checksum action.
- [x] `require_signatures` true means every planned artifact must have a signature action or an explicit unsupported error.
- [x] `require_sovereign_floor` true means every platform has at least one store-free channel.
- [x] iOS without EU sideload/alt-marketplace must produce a warning or hard error depending on policy.
- [x] Unsupported platforms must fail closed, not silently disappear.

**Tests:**

- [x] accepts direct-download floor for Linux/macOS/Windows/Android/Web;
- [x] rejects store-only platform;
- [x] documents iOS caveat;
- [x] rejects `append_only = false` unless an explicit test-only override is present;
- [ ] no PII/secrets in serialized reports.

**Acceptance:** `gear-cable plan` can fail a bad manifest before any build/publish side effect.

## Task 4 — Build artifact planning, not publishing

Implement `gear-cable plan` as a dry-run artifact graph.

**Plan output should include:**

- package name/version;
- platforms and target triples;
- build command templates;
- expected artifact paths;
- required checksum/signature/provenance/SBOM steps;
- channels and whether they are prerelease/stable;
- policy report.

**CLI example:**

```bash
gear-cable plan --manifest examples/cos-matic/gear-cable.toml --format json
```

**Acceptance:** JSON output is deterministic and snapshot-tested.

## Task 5 — Minimal local build and checksum flow

Add a local-only builder for Rust binaries.

**Scope:**

- [ ] current host target only;
- [ ] `cargo build --release --bin <binary>`;
- [ ] package as `.tar.gz` on Unix and `.zip` on Windows if easy; otherwise tarball only first;
- [ ] compute checksums;
- [ ] emit a local manifest file under `dist/`.

**Out of scope:** cross-compilation, notarization, installers, app stores.

**Acceptance:** Link Cable can build itself or a fixture binary locally and produce deterministic metadata.

## Task 6 — CI gates for Link Cable

Create `.github/workflows/ci.yml`:

- [x] checkout;
- [x] Rust toolchain with fmt/clippy;
- [x] cache;
- [x] `cargo fmt --all --check`;
- [x] `cargo clippy --workspace --all-targets --all-features` with `RUSTFLAGS=-D warnings`;
- [x] `cargo test --workspace --all-features`;
- [x] dependency audit using `cargo-audit` + `cargo-deny`;
- [x] optional compile-only portability gate for `gear-cable-core`:

```bash
rustup target add wasm32-unknown-unknown
cargo build -p gear-cable-core --target wasm32-unknown-unknown
```

**Acceptance:** PRs are blocked on format, lint, tests, audit, and portability.

## Task 7 — cos-matic consumer fixture

In Link Cable, add `examples/cos-matic/gear-cable.toml` and document how cos-matic would consume the tool.

**Important:** do not edit cos-matic yet beyond docs unless Link Cable has a green CI baseline.

**Fixture content:** start with the manifest shape above, but reduce supported platforms to what the implementation actually supports.

**Acceptance:**

```bash
cargo run -p gear-cable-cli -- plan --manifest examples/cos-matic/gear-cable.toml
```

prints a valid plan and no mutating operation runs.

## Task 8 — First integration back into cos-matic

After Link Cable has a tagged pre-release, return to cos-matic.

**Files likely changed in cos-matic:**

- `README.md`
- `docs/adr/0030-distribution-doctrine-append-only.md`
- `docs/adr/0031-supply-chain-and-sovereignty.md`
- `docs/adr/0032-native-ui-and-binding-matrix.md`
- new `gear-cable.toml` or `distribution/gear-cable.toml`
- optional CI workflow calling Link Cable in dry-run mode

**Rules:**

- [ ] ADRs should say “distribution subsystem extracted to Link Cable” once true.
- [ ] cos-matic must consume Link Cable as an external tool, not copy its internals.
- [ ] First integration is `plan`/`doctor` only; no publish from cos-matic CI.
- [ ] Keep existing cosmatic workflows green.

**Acceptance commands in cos-matic:**

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features
cargo test --workspace --all-features
./scripts/audit-deps.sh
cargo build --bin aom
./target/debug/cosmatic goals --manifest examples/minimal/harness.toml
./target/debug/cosmatic generate --check --manifest examples/minimal/harness.toml
gear-cable plan --manifest gear-cable.toml
```

## Task 9 — Pre-release workflow, dry-run first

Create a Link Cable release workflow that only builds and attaches artifacts as draft/prerelease until provenance/signing is complete.

**Required before any stable release:**

- [ ] checksums generated;
- [ ] signatures generated or explicit pre-release limitation documented;
- [ ] SBOM generated;
- [ ] SLSA provenance attached;
- [ ] release notes include any security/audit exceptions;
- [ ] no static publishing token if OIDC/keyless path exists;
- [ ] `compensate` runbook documented.

**Acceptance:** a pre-release can be installed and smoked through the direct channel before any stable pointer moves.

## Task 10 — Rename/position cleanup in cos-matic

Once Link Cable exists as a working external product:

- [ ] README architecture table should name Link Cable as the distribution substrate.
- [ ] cos-matic ADRs should retain historical context but point to Link Cable for current distribution implementation.
- [ ] Remove any TODOs implying distribution will be implemented inside `crates/orchestrator`.
- [ ] Keep orchestrator `deploy` semantics separate from Link Cable `distribute` semantics.

## Verification matrix

### Link Cable local

```bash
cargo fmt --all --check
RUSTFLAGS="-D warnings" cargo clippy --workspace --all-targets --all-features
cargo test --workspace --all-features
./scripts/audit-deps.sh
cargo build -p gear-cable-core --target wasm32-unknown-unknown
cargo run -p gear-cable-cli -- doctor
cargo run -p gear-cable-cli -- plan --manifest examples/cos-matic/gear-cable.toml
```

### cos-matic after integration

```bash
cargo fmt --all --check
RUSTFLAGS="-D warnings" cargo clippy --workspace --all-targets --all-features
cargo test --workspace --all-features
./scripts/audit-deps.sh
./target/debug/cosmatic generate --check --manifest examples/minimal/harness.toml
```

## Out of scope for the first extraction

- Native mobile/desktop UI implementation.
- App Store / Play Store production publishing.
- Windows installer signing.
- macOS notarization automation.
- F-Droid repository operation.
- OCI registry support.
- Self-updater that mutates a user installation.
- Replacing cos-matic deploy/orchestrator commands.

## Definition of done for extraction v0

- [x] Link Cable repository has green CI.
- [x] Link Cable has docs/ADRs for the extracted doctrine.
- [x] `gear-cable-core` validates a release manifest and sovereign-floor policy.
- [x] `gear-cable-cli plan` works on an cos-matic fixture.
- [ ] A local build/checksum flow exists for at least one host platform.
- [x] cos-matic references Link Cable as the distribution substrate without importing distribution internals.
- [x] No publish command can run accidentally without explicit opt-in.

## Known risks and mitigations

- **Over-abstraction:** keep first implementation to plan + local build + direct channel.
- **Supply-chain theater:** fail closed when signatures/provenance are required but unavailable.
- **Secret leakage:** no tokens in config; audit records must exclude usernames, URLs with tokens, and raw env vars.
- **Semantic confusion with deploy:** reserve rollback for deploy; distribution uses compensate.
- **cos-matic regression:** integrate only after Link Cable pre-release and run full cosmatic gates.
- **Platform sprawl:** add platforms only when a smoke test exists.
