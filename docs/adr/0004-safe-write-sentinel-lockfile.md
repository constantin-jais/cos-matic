# ADR-0004 — Safe-write sentinel: out-of-band lockfile

- Status: accepted
- Date: 2026-06-27

## Context

The distinctive guarantee is _never clobber a hand-edited generated file_. To
know whether a generated file was edited by a human, we must record what the
tool last wrote. Options considered:

1. **In-band header** comment in each generated file
   (`<!-- agent-o-matic:blake3:<hash> -->`).
2. **Out-of-band lockfile** (`.harness/lock.toml`) mapping each generated path
   to the BLAKE3 hash of the content the tool last wrote.
3. Hybrid: header where the format allows comments, plus a lockfile.

## Decision

**Option 2: an out-of-band lockfile `.harness/lock.toml`.**

On write, for each target path:

- If the path does not exist → write it, record `path -> blake3(content)`.
- If it exists and `blake3(current_file) == lock[path]` → it is tool-owned and
  unchanged by a human. Re-render: if the new content hash equals the lock
  hash, it is a **no-op** (idempotent); otherwise overwrite and update the lock.
- If it exists and `blake3(current_file) != lock[path]` (or no lock entry) →
  a human edited it (or it predates the tool). **Refuse**, with a clear message,
  unless `--force` is given.

## Rationale

- Works uniformly for _every_ file type, including `settings.json` and other
  JSON where comments are illegal. The in-band header would force per-format
  special cases on day one.
- Keeps generated files pristine (no tool metadata injected into the output a
  human reads), which matters because these files are themselves prompts fed to
  LLMs — extra metadata is noise and risks content-filtering surprises.
- Centralized state is easy to audit and to diff in review.

## Consequences

- The lockfile is committed to version control; it is the source of truth for
  drift detection (ADR-0005 / Phase 5) as well as clobber protection.
- Hashes and lockfile paths are repo-relative; never absolute/machine-local.
- Trade-off accepted: the file↔hash link is not visible _inside_ each generated
  file. Mitigated by the lockfile being small, human-readable TOML.
- The hybrid (option 3) was rejected for the first phases as gold-plating: two
  sources of truth to keep consistent, for marginal benefit.
