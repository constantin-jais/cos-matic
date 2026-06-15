# CI templates

Drop-in CI that turns cos-matic's drift detection and goals into an enforced
gate (ADR-0010).

## `cos-matic.yml` (GitHub Actions)

Copy it to `.github/workflows/cos-matic.yml`. It:

1. installs `aom`,
2. runs `cosmatic goals` (hard gates + observability report),
3. runs `cosmatic generate --check`, which fails if any committed output (`AGENTS.md`,
   `CLAUDE.md`, `.cursor/rules/*`, …) has drifted from `harness.toml` or a domain
   file.

For this to work, **commit your generated outputs and `.harness/lock.toml`**; they
are the golden files the gate checks against. (`.harness/audit.jsonl` carries
timestamps — keep it gitignored.)

## Other CI systems

The gate is just two commands — port them anywhere:

```sh
cosmatic goals            # nonzero exit if a hard gate fails
cosmatic generate --check # nonzero exit if outputs drifted from the source
```
