# ADR-0002 — Implementation language: Rust

- Status: accepted
- Date: 2026-06-27

## Context

The tool is a CLI that parses a declarative source, builds an intermediate
representation, and writes files deterministically with checksum guarantees.
Candidate languages considered: Rust, Go (what `ai-rulez` uses), TypeScript/Bun,
Zig.

## Decision

**Rust.**

## Rationale (on the project's 4 decision axes + the goal)

- **Security/correctness (axis 1):** a strong type system and exhaustive
  `match` let the compiler enforce invariants of the IR and the safe-write
  state machine at compile time. The wedge is _determinism and auditability_ —
  the language's rigor _is_ the message.
- **Quality (axis 2):** single static binary, `clippy` as a strict gate,
  ownership makes the write path's state explicit.
- **Performance (axis 3):** not the bottleneck here, but free.
- **Learning goal:** modeling a compiler IR, error diagnostics, and a
  state machine in Rust is exactly the kind of deep, teachable material wanted.

## Trade-offs accepted

- Slower to author and longer compile times than Go/TS.
- Smaller contributor pool than Go for a devtool — acceptable because adoption
  is not the goal (see ADR: positioning-and-why-build).
- Go would have eased multi-ecosystem distribution (`ai-rulez`'s approach);
  deferred, not needed for a learning artifact.
