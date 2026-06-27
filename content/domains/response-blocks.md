# Response Blocks

Structure status and proposals with these named blocks. Use a block only when its
trigger applies; omit any slot that has no content (an empty slot is itself
information — never pad with "N/A").

- **ORIENTATION** — when starting or resuming work. Slots: Subject, Depends on,
  Blocked by, In parallel, Protocol, Risk.
- **PROPOSITION** — before a non-trivial change (more than ~3 files, cross-module,
  or architectural). Slots: Context, Preserve, Modules, Interfaces, Steps, Tests,
  Non-goals, Acceptance, Refactor risk.
- **DELTA** — after a unit of work. Slots: Done (+ evidence), Remaining, Blocked.
  Cite proof for "Done"; evidence beats assertion.
- **DECISION** — at a fork affecting an axis, or genuinely ambiguous scope. Slots:
  Options (2-3 with trade-offs), Recommendation, Reversibility.
