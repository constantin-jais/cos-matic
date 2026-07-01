# ADR-0025 — North-star: trustworthy, self-hostable autonomous code-ops

## Status

Accepted (2026-06-28).

## Context

Autonomous code-ops invites justified skepticism. A raw-autonomy pitch — _we
merge and deploy on our own_ — reads as a liability when there is no way to
audit a decision, contain a blast radius, or self-host the machinery. The
incumbents chase capability and hide their limits behind opaque SaaS or
hyperscaler lock-in.

The orchestrator (ADR: workspace-and-orchestrator-charter) already delivers the
machinery: an end-to-end loop with evidence-gated merge, canary-to-rollback
deploy, and a bounded fixer hand-off. But machinery is not the discriminator —
everyone is building machinery. This ADR fixes what the project optimizes for,
so every later decision has a cap to measure against.

## Decision

Position on **auditability, self-hostability, and legibility**, not on raw
capability. The discriminator is not autonomy — it is _autonomy you can audit,
bound, and run on your own infra_. That shapes an earned ladder of trust:

- **L0 — compile-only.** Generate the agent config files. Zero autonomy, zero
  risk; the solo on-ramp. Already shipped (safe-write + drift detection).
- **L1 — bounded dispatch.** A fixer attempts a fix in an isolated worktree
  (ADR: dispatch-bounded-fixer); a human gates and merges. Autonomy in the
  _attempt_, never in the merge. Shipped.
- **L2 — gated loop.** The full loop runs (dispatch → publish → automerge on
  green evidence only, ADR: autonomous-merge → deploy), but behind an
  approve-before-merge gate and a typed audit. Proven live with a stub fixer.
- **L3 — trusted autonomy.** Full auto under a strict sandbox + policy. Requires
  L2 stable, a hardened fixer isolation boundary (today: a worktree +
  allow-listed Bash, ADR: workspace-and-orchestrator-charter; target: gVisor →
  Firecracker), and the operator's explicit grant.

**The ladder is earned, not a menu.** Each level ships and is usable in
isolation; a team that wants to stay at L1 forever may. L3 only has value
because L0–L2 make it credible — the safety, not the spectacle, is the point.

**Persona: solo → team.** The compiler half (ADR: positioning-and-why-build) is
the top of the funnel — zero-risk, immediately useful, and the thing that builds
trust before any autonomy. A solo dev enters at L0; the platform team is the
target at L2–L3 (multi-repo policy, audit, incident routing). The compiler is
not dead weight; it is the ramp.

**Motive: usefulness and forkability, not revenue.** This reframes the value.
Trust, legibility, and self-hostability _are_ the product, not a go-to-market
wedge:

- Every non-obvious decision is an ADR — a reader can fork with full context.
- The architecture is a single Rust binary, no external services, no SaaS
  (ADR: workspace-and-orchestrator-charter).
- The safety envelope (kill-switch, scope-fence, circuit-breaker, zero-PII
  audit, fail-closed) is a built-in boundary, not a feature you beg a vendor
  for (ADR: end-to-end-loop).

**Honesty bar.** The docs must mark exactly where "built and proven" ends and
"designed, not yet built" begins. Today: L0–L1 and L2-with-a-stub-fixer are
proven live; L3 isolation (gVisor/Firecracker) and a typed policy are targets,
not implementations.

## Consequences

- Autonomy stops being a binary pitch and becomes a graduated ladder tied to
  proof and control. The framing everywhere becomes "start at zero risk, earn
  autonomy as confidence grows."
- The L0→L3 ladder shapes the roadmap, the docs, and the `bolt-cosmatic init` presets.
  Each level is a distinct, shippable milestone.
- Success is forks and self-hosted instances, not SaaS seats. A fork that bends
  the policy to its own domain is a win.
- Technical choices optimize for self-hostability and auditability (static
  binary, no external service, typed policy) over operational convenience or
  feature velocity. Where the two conflict, trust wins — that is the whole point
  (ADR: operate-loop-as-scoped-ci-bot).
- The concrete seams that make the ladder real — a Forge trait, a fixer
  isolation boundary, a typed policy, durable run-state — are specified
  separately so this ADR stays about the _why_ (see the architecture-targets
  ADR for the _how_).
