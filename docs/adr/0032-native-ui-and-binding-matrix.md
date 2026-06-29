# ADR-0032 — Native UI strategy and binding matrix

## Status

Proposed (2026-06-29). Fixes the per-platform UI and binding choices over the core.

## Context

With the logic settled as a single bound Rust core (ADR:
portability-rust-core-bind-not-reimplement), the open question is the UI layer.
The stated product goal is **native apps with the best performance on every
device**, each version shipped in a platform-optimized format. Performance is a
load-bearing requirement here, not a tie-breaker.

Three UI strategies were weighed: truly-native per platform; one shared
Rust-native UI (Slint/Dioxus/egui) compiled everywhere; or a hybrid. The choice
is **truly-native per platform**.

## Decision

**One Rust core, a truly-native UI per platform, generated bindings between
them.** The N UI codebases are accepted deliberately: **N native UIs are not N
logic implementations** — the determinism contract lives entirely in the single
core (ADR: portability-rust-core-bind-not-reimplement), and UI is the one
legitimately divergent layer.

### Binding matrix

| Platform | Native UI        | Binding from `aom-core`          | Packaging                      | Sovereign floor                      |
| -------- | ---------------- | -------------------------------- | ------------------------------ | ------------------------------------ |
| iOS      | SwiftUI          | UniFFI → Swift                   | `.ipa`                         | ⚠️ App Store global; EU DMA sideload |
| macOS    | SwiftUI/AppKit   | UniFFI → Swift (shared with iOS) | `.app`/`.dmg` notarized        | direct signed `.dmg`                 |
| Android  | Jetpack Compose  | UniFFI → Kotlin                  | `.aab`/`.apk`                  | F-Droid + direct APK                 |
| Windows  | WinUI 3 / .NET   | WIT→C# _or_ `uniffi-bindgen-cs`  | signed `.msix`/`.exe`          | direct signed installer              |
| Linux    | GTK4 (`gtk4-rs`) | **direct Rust link (no FFI)**    | AppImage/`.deb`/`.rpm`/Flatpak | self-hosted repo + AppImage          |
| Web      | web (TS)         | wasm-bindgen → JS/TS             | WASM bundle                    | self-hostable static                 |

Three facts shape the matrix:

- **Apple platforms share** one Swift binding and most of a SwiftUI codebase — an
  efficiency internal to "truly native".
- **Linux in `gtk4-rs` links the core directly** — same language, zero FFI
  boundary, the lowest-risk vertical.
- **Windows native (.NET) is the only target needing a C# binding** — via the
  WIT/Component-Model path or the community `uniffi-bindgen-cs`. Maturity is
  verified at integration, not assumed (per the "verify before prescribing" rule).

### Performance reinforces the choice

Native UI (no Electron/webview) + a Rust core at native speed via FFI (negligible
per-invocation overhead) + per-architecture optimized builds (aarch64/x86_64,
`target-cpu`/SIMD features, LTO) + platform-optimized packaging is the
configuration that delivers "best performance regardless of device". WASM in the
browser carries a small marshalling cost, imperceptible on a millisecond compile.

A **cross-runtime parity test** guarantees that performance does not cost
correctness: the same fixtures through the native `compile()`, the WASM build, and
a native app must produce byte-identical output — the load-bearing guarantee of
"build once = same behavior everywhere".

## Consequences

- **Max native feel and performance,** at the cost of N UI codebases — the
  divergent layer, accepted explicitly.
- **The binding ABI is specified once:** the types crossing every boundary are
  `RenderedFile { path, content }` plus a serializable diagnostic DTO (miette
  spans do not cross FFI); see the seam in `aom-core`.
- **The error contract is uniform** across Swift/Kotlin/C#/JS via that DTO.
- **Linux and Android are the first verticals** (D1): Linux for the zero-FFI
  link, Android to exercise the UniFFI boundary against a fully sovereign floor.
- **Honest dependency:** truly-native means a per-platform UI toolchain (Xcode,
  Android SDK, WinUI, GTK). The core insulates them from the logic, but the UI
  build matrix is real and owned per platform.
