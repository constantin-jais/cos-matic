# Code Style

- Explicit is better than implicit; readability beats concision.
- Prefer duplication over the wrong abstraction (rule of three: extract a shared
  abstraction only on the third occurrence).
- Comments explain the _why_, not the _what_. Annotate deliberate debt with a
  reason.
- Match the surrounding code: its naming, its idioms, its comment density. New
  code should read like the code already there.
- Keep units small and single-purpose. When a file grows large, that is a signal
  it is doing too much.
