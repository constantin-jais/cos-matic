# ADR-0031 — Supply-chain and sovereignty for distribution

## Status

Proposed (2026-06-29). Sets the security and sovereignty floor for every release.

## Context

A large distribution surface adds two things the runtime safety envelope was
never designed to cover:

1. **Static publishing credentials.** Registry tokens and store signing keys are
   secrets that, if leaked, let an attacker publish under your identity —
   bypassing every runtime gate, because the key is checked at publish time, not
   at policy-decision time. Six-plus channels × a few secrets each is a real
   key-management and breach-response surface.
2. **Artifact provenance.** Because publishing is irreversible (ADR:
   distribution-doctrine-append-only), an artifact's only durable guarantee is
   that it is traceable, signed, and reproducible from source.

Separately, "run everywhere" collides with the sovereignty stance (no US
hyperscalers): npm, the VS Code Marketplace, the App Store, and the Play Store
are US-corporation gatekeepers. This is a genuine trade-off, not a hedge.

## Decision

### Keyless by default — dissolve the static-secret surface

Prefer credentials that **do not exist at rest**:

- **sigstore/cosign keyless** for binaries and OCI images — the OIDC identity is
  the credential; there is no long-lived key to leak or rotate.
- **OIDC trusted-publishing** to crates.io and npm — no stored registry token.

Where a static secret is unavoidable (the app stores), it is isolated, scoped to
one channel, audited on use, and gated. The objection "signing keys are an
uncovered attack surface" is answered by _removing most of the keys_.

### Prove every artifact

- **SLSA provenance** (≥ L2, target L3 via isolated reusable workflows): artifact
  ⇐ commit ⇐ CI run.
- **SBOM** via `cargo-cyclonedx`, embedded with `cargo-auditable`.
- **Reproducible builds of the core** (`SOURCE_DATE_EPOCH`, `--remap-path-prefix`,
  `CARGO_INCREMENTAL=0`, pinned `rust-toolchain.toml`). Reproducibility is
  committed _at the core_, honestly **not** at post-store artifacts (App Thinning
  and multi-APK splitting are non-deterministic by design).

### Sovereignty as a typed invariant, enforced as a gate

Store channels are permitted **for reach** — but only if a **store-free install
path exists for every supported platform**. This is not a preference; it is a
**blocking CI gate**: a release fails if any platform's channel set has only a
store and no floor.

| Platform | Sovereign floor (no store)                                                |
| -------- | ------------------------------------------------------------------------- |
| macOS    | signed/notarized `.dmg` direct download                                   |
| Android  | F-Droid + direct APK                                                      |
| Windows  | signed installer direct download                                          |
| Linux    | self-hosted repo + AppImage                                               |
| Web      | self-hostable static WASM bundle                                          |
| iOS      | ⚠️ EU only (DMA sideload / alt-marketplace); no sovereign floor elsewhere |

The iOS caveat is stated, not hidden: outside the EU, Apple is a hard monopoly
and the floor does not exist. Enabling a store channel records the operator's
explicit acceptance in typed policy (`[distribution]`), consistent with the
scope-fence (ADR: workspace-and-orchestrator-charter).

## Consequences

- **Most releases carry no long-lived secret** — the breach surface is the few
  unavoidable store keys, isolated and audited.
- **Every artifact is signed, attested, and (at the core) reproducible** — the
  only honest answer to irreversible publishing.
- **The sovereign floor is mechanically guaranteed,** not aspirational: the gate
  refuses a store-only platform.
- **iOS is the documented exception** — full sovereignty there is EU-gated by the
  DMA; this is surfaced to the operator, not papered over.
- **Self-hostable channels stay first-class:** OCI (Harbor), a git/crate index,
  F-Droid, and direct signed download cover the floor without any US gatekeeper.
