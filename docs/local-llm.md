# Local LLM testing with LM Studio

Local LLM tests should not require provider secrets and should not be wired into
public GitHub Actions. The recommended local model endpoint is LM Studio on the
operator workstation.

## Recommended local model

- Provider/runtime: LM Studio
- Base URL: `http://127.0.0.1:1234/v1`
- Model: `google/gemma-4-26b-a4b-qat`

## Smoke test

Start LM Studio, load the model, enable the local server, then run:

```sh
scripts/local-llm-smoke.sh
```

Expected output:

```text
bolt local llm ok
```

The script defaults to:

```sh
LM_STUDIO_BASE_URL=http://127.0.0.1:1234/v1
LM_STUDIO_MODEL=google/gemma-4-26b-a4b-qat
```

Override those environment variables only for local experiments.

## Evidence

Local smoke evidence is recorded in [`docs/evidence/2026-07-01-local-llm-smoke.md`](evidence/2026-07-01-local-llm-smoke.md).

## Current integration boundary

`bolt-cosmatic loop --dry-run` and `fixer=stub` do not use an LLM.

The current write-capable fixer implementation is a Claude Code CLI seam retained
for historical/live-boundary work. It is not the recommended public demo path and
is not evidence that an Anthropic key exists or is required.

LM Studio/Gemma is the preferred local LLM test target, but it is not yet a
write-capable fixer backend. Do not expose `127.0.0.1:1234` through public CI.

## Secret rules

- LM Studio local tests require no repository secret.
- Do not store local model URLs or prompts in GitHub secrets.
- Do not commit generated local transcripts unless they are scrubbed evidence.
- If a cloud provider fixer is intentionally tested in the future, rotate that
  provider key separately; otherwise there is no provider key to rotate.
