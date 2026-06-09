# ADR-0030 — Distribution doctrine: append-only, forward-only, compensate-not-rollback

## Status

Proposed (2026-06-29). Governs every channel the distribution subsystem touches.

## Context

The safety envelope was built for **deployments**, which are reversible: a bad
deploy is recovered by redeploying the previous version, and the cardinal rule is
"failed smoke ⇒ roll back" (ADR: deploy-canary-smoke-rollback). Distribution is
not deployment. **Publishing is append-only and irreversible:**

- a yanked crate is not deleted — existing `Cargo.lock` files still resolve it;
- an npm version can be deprecated, not truly unpublished after the grace window;
- a pushed tag / released artifact is effectively permanent;
- an app-store submission enters an archive and a review queue.

Reusing the deploy abstraction unchanged would be a category error: "canary →
smoke → promote → _rollback_" implies a reversibility that registries do not
offer. A design that hides this invites the assumption that a publish can be
undone — the most dangerous false belief in a release pipeline.

## Decision

Distribution gets its **own doctrine**, sharing the envelope's _principles_
(evidence gate, scope-fence, circuit-breaker, zero-PII audit, kill-switch) but
**not** the deploy _flow_.

1. **Append-only, forward-only.** The stable pointer (`latest`, the released
   tag) only ever _advances onto_ an artifact that already carries green
   evidence. Nothing is exposed before evidence except an explicit _pre-release_
   channel (crate `-rc`, `npm --tag canary`, a draft/pre-release).
2. **`promote` is a pointer move, not a re-publish.** Promotion adds the stable
   tag to an already-published pre-release; it does not re-upload.
3. **`compensate`, never `rollback`.** The recovery action is yank/deprecate —
   best-effort, slower, and _not_ presented as a rollback. The trait says
   `compensate` precisely so no caller mistakes it for "undo".
4. **The envelope's weight moves to pre-publish.** Because nothing can be undone,
   the gate is _stricter before_ the publish: a mandatory dry-run (`plan`), a
   hard evidence gate, and **immutable provenance** attached at build time (ADR:
   supply-chain-and-sovereignty) so an irreversible artifact is at least always
   traceable to its source.

`Distributor` is therefore a **sibling** of `Deployer`, not a mirror: a narrow
trait (`plan` / `publish_prerelease` / `smoke` / `promote` / `compensate`) that is
the genuine common denominator of forward-only registries. Channel-specific
richness (staged-percentage rollout on the app stores, retag semantics on OCI)
lives in the _impl_, never in the trait — the lesson from multi-forge: do not
abstract incompatible systems into one leaky interface (ADR:
architecture-targets-seams-isolation-durability).

## Consequences

- **No rollback theater.** The pipeline never claims to undo a publish; the audit
  records `promoted` or `compensated`, not `rolled_back`.
- **Pre-release is a real channel,** not a fiction — `smoke` verifies the
  pre-release artifact is actually installable before the pointer moves.
- **Provenance is mandatory, not optional,** because it is the only durable
  guarantee left once an artifact is irreversibly public.
- **Narrow trait, fat impls.** Adding the App Store does not widen the trait; it
  adds an impl whose extra capabilities (rollout %, review queue) stay local.
- **Zero-PII audit** to `.harness/distribute.jsonl`: action, repo, channel,
  version, outcome, timestamp — no tokens, URLs, or usernames (ADR:
  workspace-and-orchestrator-charter).
