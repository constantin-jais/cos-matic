# Agent-O-Matic

> A deterministic, agent-agnostic **configuration compiler**: one declarative
> source → configuration for many AI coding agents, with **safe-write** and
> **drift detection**.

**Status: early, work-in-progress (Phase 1).** Built clean-room as a
learning/teaching artifact — see [`docs/adr/`](docs/adr/) for the reasoning
behind every decision.

## What it does

You write one source-of-truth: a `harness.toml` manifest that declares reusable
instruction **domains**, **profiles** (named subsets), and **targets** (per-agent
outputs). Domain prose lives in plain Markdown files. `aom generate` compiles
that source into each agent's native config — starting with the universal
[`AGENTS.md`](https://agents.md/), with `CLAUDE.md`, Cursor, and others to come.

```toml
# harness.toml
[package]
name = "my-project"

[[domains]]
name = "code-style"
priority = 8
content_file = "domains/code-style.md"

[[profiles]]
name = "default"
domains = ["code-style"]

[[targets]]
name = "agents-md"
adapter = "universal"
output_file = "AGENTS.md"
profile = "default"
```

## What makes it different

This is **not** trying to beat the mature, excellent
[`ai-rulez`](https://github.com/Goldziher/ai-rulez) on its own ground (one
source → ~19 agents, batteries-included). It exists to explore — and teach — the
two subsystems that tool leaves implicit:

- **Safe-write.** A generated file you hand-edit is never silently clobbered.
  An out-of-band lockfile (`.harness/lock.toml`) records what the tool last
  wrote; regeneration refuses to overwrite human edits unless you pass `--force`.
- **Drift detection.** Regeneration is reproducible. `aom generate --check`
  fails (exit 1) when committed outputs diverge from the source — a CI gate.

See [ADR-0001](docs/adr/0001-positioning-and-why-build.md) for the honest
positioning.

## Build

```sh
cargo build
cargo test
```

## License

MIT — see [LICENSE](LICENSE).
