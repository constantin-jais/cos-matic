# ADR-0033 — Rust-core doctrine hardening

## Status

Proposed (2026-06-30). Normalizes the legacy agent configuration corpus into a
stricter Rust-first doctrine.

## Context

An older `claude-config-system` repository contained useful rules for Rust,
Bun/TypeScript, Zig, Biscuit, CI/CD, testing, hooks, and sovereign providers.
The content was valuable, but its shape reflected a Claude-centric and broader
polyglot operating model: Bun/TypeScript could own backend logic, Zig appeared
as a stack participant, shell scripts implemented durable automation, and live
Claude configuration could be treated as a source of truth.

The current ecosystem has a sharper target:

- Rust at the core;
- one source of truth compiled or bound to multiple surfaces;
- artifact-first release and distribution;
- sovereignty and supply-chain evidence as gates;
- TypeScript limited to web/product experience;
- no architectural ownership for Zig.

A straight migration would preserve useful knowledge but also re-import old
ambiguity.

## Decision

Adopt a **Rust-Core Doctrine Hardening** strategy.

Legacy material is not copied as-is. It is classified and normalized:

- **Promote** Rust conventions into `rust-core`.
- **Promote** provider/license/RGPD/DORA rules into `sovereign-stack`.
- **Promote** release rules into `artifact-first-release`, replacing rebuild-on-
  deploy assumptions with immutable artifact evidence.
- **Keep** Biscuit authorization as `biscuit-auth`.
- **Keep and sharpen** testing rules as `testing-strategy`.
- **Reduce** Bun/TypeScript to `web-boundary`.
- **Demote** Zig to `native-escape-hatches`; it owns no layer.
- **Reimplement** durable shell scripts as Rust commands or Wrench checks before
  they become part of the new system.

The central rule is captured in `stack-authority`:

> Rust owns every durable system capability. TypeScript owns browser-facing
> experience. Zig owns nothing architecturally. Shell is temporary glue.

Any exception requires an ADR explaining why Rust is not appropriate, how long
the exception should live, and how it will be removed or contained.

## Consequences

- The embedded content library now carries strict Rust-first built-ins that can
  be compiled into `AGENTS.md`, `CLAUDE.md`, Cursor rules, or future adapters.
- New `bolt-cosmatic init` projects inherit Rust-core doctrine by default instead of
  a generic multi-stack posture.
- TypeScript backend, durable shell, and hand-written Zig become explicit
  exceptions rather than silent defaults.
- Future automation should turn this doctrine into gates: language ownership,
  ADR-required exceptions, artifact evidence, license/source policy, and
  sovereignty checks.
- This decision intentionally narrows completeness in favor of security,
  quality, and long-term portability.
