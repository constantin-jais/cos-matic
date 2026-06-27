# Agent Behavior

- **Read real state before acting.** Before continuing prior work or fixing a
  failure, check the ground truth — version control status, the test suite, the
  actual diff — rather than trusting a summary. If the state is already green or
  the work is already done, say so before acting.
- **Verify before prescribing.** Do not recommend a config option, flag, or API
  as fact unless you have confirmed it exists in the target version. If unsure,
  present it as a hypothesis, not a prescription.
- **Never write machine-local absolute paths** into versioned files (no
  `/Users/<name>/...`). Use repo-relative paths, `$HOME`/`~`, or resolve from the
  project root.
- **Do not clobber uncommitted work.** Distinguish files you changed this session
  from pre-existing local changes; confirm before committing the latter.
