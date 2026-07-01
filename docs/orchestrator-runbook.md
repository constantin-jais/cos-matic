# Operating the autonomous loop

`bolt-cos-matic` owns the loop engine: dispatch, publish, gate, automerge, deploy
envelopes, audit records, and fail-closed behavior.

The canonical engine repository does **not** own live sandbox execution. Live
write-capable demonstrations belong in [`bolt-harness`](https://github.com/constantin-jais/bolt-harness),
which is the disposable proof bench for public CI scenarios.

## Engine repository mode

The engine repository keeps only a manual, read-only smoke workflow:

```bash
gh workflow run orchestrator-loop.yml -f issue=1 -f title="ci: autonomous loop"
```

That workflow runs:

```bash
bolt-cosmatic loop --dry-run
```

It uses the built-in `github.token` with read permissions only. It must not push
branches, open or merge PRs, call Claude, or deploy.

## Live sandbox mode

Use `bolt-harness` for live runs. Its live workflow is manual-only and must be
fenced with:

```bash
BOLT_HARNESS_SANDBOX=true
```

Live mode requires a fine-grained sandbox-only credential stored as
`BOLT_COSMATIC_BOT_TOKEN`. `ANTHROPIC_API_KEY` is needed only when deliberately
running `fixer=claude`; prefer `fixer=stub` for public demonstrations.

## Credential rules

- Dry-run: built-in `github.token`, read-only workflow permissions.
- Live: sandbox-only fine-grained PAT, never a broad human token.
- `BOLT_COSMATIC_CHECKS_TOKEN` is supplied by the workflow from `github.token`;
  do not create it as a repository secret.
- No production repositories, production logs, private URLs, secrets, tokens, or
  personal data in public evidence.

## Honest limitations

- The gate waits only within a bounded fail-closed window.
- Missing, failed, or still-pending checks refuse merge.
- Deploy commands in the public harness are no-op by default.
- A live run validates wiring and safety envelopes, not fixer quality.
