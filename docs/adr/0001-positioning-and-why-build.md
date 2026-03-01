# ADR-0001 — Positioning: why build this when `ai-rulez` already exists

- Status: accepted
- Date: 2026-06-27

## Context

The problem "compile one declarative source into configuration for many AI
coding agents" is **already solved** by a mature tool, **`ai-rulez`**
(Goldziher, v4.4.1, written in Go). It ships a TOML source with
`builtins`/`[profiles]`/`[[includes]]`, generates for ~19 agent targets,
bundles ~33 content domains and ~6 preconfigured agents, exposes an MCP
server, and is distributed via `npx`/`brew`/`pip`.

When three independent design explorations were run from scratch, they all
re-converged on the same paradigm (TOML domains/profiles/targets + safe-write +
drift). That is strong evidence the _paradigm_ is a commodity. Differentiating
on the paradigm alone is not possible.

## Decision

Build Agent-O-Matic anyway, but be honest about _why_ and what success means.

**Primary goal: learning, in order to teach.** The value is depth of
understanding gained by reconstructing the system clean-room, and the
legibility of every decision so the repo doubles as teaching material.
Market adoption is explicitly **not** the success metric.

**Intellectual wedge (where we go further than `ai-rulez`):** determinism and
safety — _safe-write_ (never clobber a hand-edited generated file), _drift
detection as a blocking CI gate_, and an auditable, reproducible write path.
`ai-rulez` does not document these; they are also the richest subsystems to
learn from.

## Consequences

- We **do not** invest in adoption-race work: no competitor-user interviews,
  no marketing positioning doc, no remote content registry, no "19 targets on
  day one", no early MCP server. Those would be gold-plating against the goal.
- We **do** invest in the parts with high learning value and that embody the
  wedge: a clean compiler pipeline, safe-write, drift, rich diagnostics.
- The README states the `ai-rulez` relationship openly. Honesty is part of the
  teaching value; pretending to be "first to do X" would be false.
- Every non-obvious choice gets an ADR. Tests are the executable spec.
