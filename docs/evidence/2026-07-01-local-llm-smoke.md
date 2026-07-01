# Evidence — local LM Studio smoke

Date: 2026-07-01

## Scenario

Local-only LM Studio chat-completions smoke test for the recommended development
LLM endpoint. This is not a GitHub Actions workflow and does not use repository
secrets.

## Inputs

- Command: `scripts/local-llm-smoke.sh`
- Base URL: `http://127.0.0.1:1234/v1`
- Model: `google/gemma-4-26b-a4b-qat`
- Expected content: `bolt local llm ok`

## Result

- Conclusion: success.
- Observed content: `bolt local llm ok`.
- Secrets configured: none.
- Provider key: none.
- Public network exposure: none; local loopback only.

## Hygiene note

This evidence stores only local smoke metadata and the non-sensitive expected
response. It does not store model reasoning traces, prompts beyond the fixed smoke
instruction, tokens, private URLs, personal data, or production logs.
