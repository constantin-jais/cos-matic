# ADR-0013 — Claude Tier-2 extensions (subagents, skills, hooks)

- Status: accepted
- Date: 2026-06-27

## Context

The tiered adapter model (ADR-0006) reserves Tier 2 for platform-native features
that have no neutral equivalent. Claude Code has several: **subagents**
(`.claude/agents/<name>.md`), **skills** (`.claude/skills/<name>/SKILL.md`), and
**hooks** (in `.claude/settings.json`). Until now the `claude` adapter emitted
only `CLAUDE.md` (Tier 1.5).

## Decision

The `claude` adapter emits these Tier-2 constructs as additional files. They are
declared as **fields on the `claude` target**, so only the claude adapter reads
them — the neutral domain/profile core stays unpolluted:

```toml
[[targets]]
name = "claude"
adapter = "claude"
output_file = "CLAUDE.md"
profile = "default"

[[targets.subagents]]
name = "reviewer"
description = "Reviews a diff against the project's axes."
model = "sonnet"            # optional
tools = ["Read", "Grep"]    # optional
prompt = "You are a focused code reviewer. ..."

[[targets.skills]]
name = "release"
description = "Cut a release."
content = "Steps: 1. ..."

[[targets.hooks]]
event = "PostToolUse"
command = "cargo fmt"
```

Outputs: `.claude/agents/<name>.md` (frontmatter + prompt), `.claude/skills/<name>/SKILL.md`
(frontmatter + content), and `.claude/settings.json` (a `hooks` block) when any
hook is declared. Every file flows through the same safe-write + drift path.

**Gating (ADR-0007):** `Feature` gains `Subagents`, `Hooks`, `Skills`. The claude
adapter supports them; the others do not. If a target whose adapter lacks the
capability declares these fields, the engine warns (graceful degradation) — the
fields are ignored, not an error.

Names (subagent / skill) are validated as safe identifiers, the same rule as
domain names, since they become directory and file names.

## Consequences

- Adding a Claude-only construct is local to the claude adapter; no other adapter
  changes. Other agents' equivalents (e.g. Gemini subagents) would be their own
  adapters' concern.
- `settings.json` is emitted **tool-owned**: safe-write refuses to overwrite a
  hand-edited one. Merging into a user's existing `settings.json` (preserving
  unrelated keys) is a deliberate future enhancement, not in this phase.
- Content is inline in the manifest for now; a `*_file` reference (like domains)
  could come later if prompts grow large.
