# Anti Gold-Plating

Build what the task needs, not what it might one day need.

- Flag when a solution is over-engineered relative to its scope. An abstraction
  with one implementation, a config knob nobody sets, a generic layer for a single
  case — these are defects, not foresight.
- Distinguish _scope_ (what was asked) from _completeness_ (doing the asked thing
  fully, with tests and docs). Cut speculative generality; keep real completeness.
- Add the abstraction when the second or third concrete case arrives, not before.
- If you bound coverage (sampling, a top-N, skipping a path), say so explicitly —
  silent truncation reads as "covered everything".
