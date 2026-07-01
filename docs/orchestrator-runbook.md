# Operating the autonomous loop

The orchestrator is built to run **unattended** behind its safety envelope, so the
right principal for its outward actions is a **scoped service identity**, never a
human's full-privilege personal token. The `orchestrator-loop` workflow is that
identity: the ephemeral `github-actions[bot]`, narrowed by a `permissions:` block
to exactly what the loop touches (ADR: operate-loop-as-scoped-ci-bot).

## Credentials — what to provide, and why

| Mode        | GitHub credential                                       | Why                                                                                                                                                 | Anthropic                                                   |
| ----------- | ------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------- |
| **dry-run** | built-in `github.token` (scoped by `permissions:`)      | read-only: resolves the repo and queries the merge gate; nothing outward                                                                            | none                                                        |
| **live**    | a **fine-grained PAT** stored as secret `BOLT_COSMATIC_BOT_TOKEN` | a branch pushed by `github.token` does **not** trigger CI, so the gate would never see green checks — the PAT makes the bot's push start the checks | `ANTHROPIC_API_KEY` only when using `fixer=claude` |

This is the answer to the "scoped token" question: for read-only there is **no PAT
to manage** — the workflow `permissions:` block is the whole scope. The PAT exists
only for the live path, and is scoped to **`contents` + `issues` + `pull_requests`
on the sandbox repo alone** — not your account. `BOLT_COSMATIC_CHECKS_TOKEN` is not a secret
to create manually; the workflow maps the runner's scoped `github.token` to that
environment variable for check-read access.

## One-time sandbox setup (live only)

Never run live against the real repo. Use a throwaway fork:

1. Fork (or create) a disposable repo with the same CI workflows.
2. Flag it as a sandbox so the workflow's guard allows live runs:
   ```bash
   gh variable set BOLT_HARNESS_SANDBOX --body true --repo <you>/<sandbox>
   ```
3. Create a **fine-grained PAT** (GitHub → Settings → Developer settings →
   fine-grained tokens): repository access = the sandbox only; permissions =
   Contents (RW), Issues (RW), Pull requests (RW). Store it:
   ```bash
   gh secret set BOLT_COSMATIC_BOT_TOKEN --repo <you>/<sandbox>
   ```
   If you will run the real Claude fixer (`-f fixer=claude`), also store:
   ```bash
   gh secret set ANTHROPIC_API_KEY --repo <you>/<sandbox>
   ```

## Running it

```bash
# Safe smoke — read-only, runs as github-actions[bot], no PAT, no Claude.
gh workflow run orchestrator-loop.yml -f issue=1 -f mode=dry-run

# Full autonomous loop — sandbox only, after the setup above.
gh workflow run orchestrator-loop.yml -f issue=<n> -f mode=live
```

Watch it in the Actions tab. The Bash-capable fixer runs **inside the ephemeral
runner**, which contains the one genuinely dangerous capability (`--allowedTools
Bash`) to a throwaway machine rather than your laptop.

## Honest limitations (what a live run will and won't do)

- **The gate waits, but only within a bounded fail-closed window.** After publish,
  `automerge` uses the `ForgeGate`/octocrab check-runs path to poll until checks
  settle. Green checks can let the loop complete in one run; failed, missing, or
  still-pending checks at the timeout return `Unknown`/refusal rather than merge.
- **Deploy is a no-op.** The `BOLT_COSMATIC_DEPLOY_*` commands are all `true`; the stage
  exercises wiring, never real infrastructure.
- **The fixer may no-op.** On an issue with no real bug, the headless Claude may
  make a trivial change or none — the test is that the mechanism runs end to end,
  not that it ships a meaningful fix.

These are deliberately surfaced rather than hidden: a green dry-run proves the
identity and wiring; a live run proves the envelope (scope-fence, kill-switch via
`BOLT_COSMATIC_LOOP_DISABLED`, circuit-breaker, zero-PII audit) holds under a real bot.
