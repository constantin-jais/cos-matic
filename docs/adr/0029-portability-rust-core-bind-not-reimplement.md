# ADR-0029 — Portability: Rust core, bind don't reimplement

## Status

Proposed (2026-06-29). Sets the portability contract for every non-Rust surface.

## Context

The compiler (`crates/core`) is the portable asset: pure, synchronous, and
dependency-light (`blake3`, `serde`, `toml`, `miette`) — it compiles to `wasm32`
with no native dependency. The orchestrator is the opposite: async,
`octocrab`/`tokio`, GitHub-coupled, holder of the safety envelope. "Build once,
run everywhere" therefore applies to the compiler, not the loop — the loop is a
server-side daemon, never an edge runtime (ADR: architecture-targets-seams-isolation-durability).

There are two ways to put the compiler on many platforms (web, desktop, mobile):

1. **Bind one Rust core.** Compile the logic once; generate a thin facade per
   target (WASM, FFI); write native UI on top.
2. **Reimplement the logic per platform.** Port the parse → resolve → merge →
   render pipeline to Swift, Kotlin, TypeScript, etc.

The standing objection to (2) — "N implementations drift and cost N× to
maintain" — has been explicitly raised as _weakened by continuous AI testing_:
if agents re-run a conformance suite on every deploy, drift is caught
automatically. That premise is real but narrow, and this ADR records why it does
not flip the decision.

## Decision

**Rust core, compiled once, bound via generated facades. The logic is never
reimplemented.** The product decomposes into three layers, and only the top one
is allowed to be per-platform:

1. **Logic** — one pure Rust core (`compile()` → `Vec<RenderedFile>`). Single
   source of truth.
2. **Bindings** — _generated_, not hand-written: UniFFI (Swift/Kotlin),
   WIT/Component-Model (WASM/edge/C#), wasm-bindgen (browser).
3. **UI** — native per platform. The one layer that is legitimately divergent;
   pixels carry no determinism contract.

### Why (2) loses on every axis

The product is a _deterministic compiler_: same input → byte-identical output,
everywhere. That is a semantic contract over an **unbounded input space**
(arbitrary TOML + Markdown). Mapped to the decision axes:

- **Security (axis 1).** One core = one surface to audit, one place to harden
  against manifest-content injection. N implementations = N audit surfaces, each
  with its own stdlib, dependency tree, and memory model. Continuous testing
  does not help: security is the adversarial input the suite did not imagine.
- **Quality (axis 2).** Determinism is _structural_ with one core (it is the
  same code). With N implementations it must be _re-established_ per language and
  is only ever _sampled_ by tests — two implementations passing the same finite
  suite can still diverge on the next manifest a user writes. Byte-identity also
  forces pinning Unicode normalization, float formatting, map iteration order,
  TOML datetime parsing, and sort stability _per language_ — specification work
  that testing reveals only _after_ divergence.
- **Performance (axis 3).** A wash. The compile is milliseconds; FFI overhead is
  negligible per invocation, and native Rust out-parses a Swift/Kotlin/TS port.
- **Completeness (axis 4).** One conformance suite, one set of API docs, one
  audit — not N.

### Why "continuous AI testing" does not rescue (2)

It reduces exactly one cost — _drift detection between implementations_. It does
nothing for divergence on the unbounded input space (tests are samples, not a
proof over the space), the per-language byte-exact specification, the ×N audit
surface, or the O(N) cost of every future change (a new adapter or manifest
field is written once for the core, N times for N ports). Evolution is O(1) here
and O(N) there; testing frequency does not change that exponent.

### The dogfood argument

bolt-cos-matic exists to eliminate one anti-pattern: _"one source of truth,
compiled to many targets — never hand-maintain the N outputs."_ Reimplementing
the tool itself N times in native languages would apply that very anti-pattern
to the tool. The architecture must eat its own dogfood: the Rust core is the
manifest; the bindings are the compiled targets.

### Precedent

Signal, 1Password, Bitwarden (crypto core), and Mozilla application-services —
which created UniFFI for exactly this problem — all face this decision and all
chose a shared Rust/native core with native UI, not N reimplementations.

## Consequences

- **One seam to build:** a pure, I/O-free `compile()` in `bolt-cos-matic core` returning
  `Vec<RenderedFile>` (the abstraction already exists, ADR: adapter-output-model).
  All filesystem work stays in a thin native wrapper. This improves testability
  today, independent of any binding.
- **Generated, not written, bindings.** The per-platform facade is codegen; the
  native work is confined to UI. The binding ABI (types crossing the boundary,
  serializable diagnostics) is specified once — see ADR: native-ui-and-binding-matrix.
- **The orchestrator does not cross.** It stays a server-side daemon; "drive the
  loop from a phone" is a thin API client, not the loop on the device.
- **A CI gate keeps the core portable:** `cargo build -p bolt-cos-matic core --target
wasm32-unknown-unknown` (compile-only). The day a native-only dependency
  enters the core, CI breaks — portability becomes a gate, not a hope.
- **Honest limit.** Native UI is per-platform and that surface is real (ADR:
  native-ui-and-binding-matrix). But N UI codebases are not N logic
  implementations; the determinism contract lives entirely in the single core.
