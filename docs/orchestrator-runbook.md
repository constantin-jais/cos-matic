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

Live mode in the public harness uses the deterministic stub fixer and requires a
fine-grained sandbox-only credential stored as `BOLT_COSMATIC_BOT_TOKEN`.
That token is for bounded branch-owning autonomy (ADR-0035): create issues/PRs,
push candidate branches, and clean up agent-owned branches — never repository
administration, secrets, settings, or protected branches.

Local LLM smoke tests should use LM Studio with `google/gemma-4-26b-a4b-qat` at
`http://127.0.0.1:1234`; see [`docs/local-llm.md`](local-llm.md). This local path
requires no repository secret and is not exposed through public CI.

## Credential rules

- Dry-run: built-in `github.token`, read-only workflow permissions.
- Live public harness: sandbox-only fine-grained PAT, never a broad human token.
- Branch autonomy: only branches inside the agent-owned namespace may be created,
  pushed, or deleted. The loop fails closed before `git push`, automerge, or
  deploy if dispatch returns a non-owned branch.
- Branch GC: deletion must be planned from ownership metadata plus expired TTL;
  the GC kill-switch maps to `BOLT_COSMATIC_GC_DISABLED=1` at the live boundary.
- Candidate selection: multi-attempt autonomy must first reject non-green or
  sensitive candidates, then prefer small low-risk diffs with coverage evidence.
- Local LLM smoke: no GitHub secret; LM Studio runs on the operator workstation.
- `BOLT_COSMATIC_CHECKS_TOKEN` is supplied by the workflow from `github.token`;
  do not create it as a repository secret.
- No production repositories, production logs, private URLs, secrets, tokens, or
  personal data in public evidence.

## Honest limitations

- The gate waits only within a bounded fail-closed window.
- Missing, failed, or still-pending checks refuse merge.
- Deploy commands in the public harness are no-op by default.
- A live run validates wiring and safety envelopes, not fixer quality.
