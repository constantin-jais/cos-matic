#!/usr/bin/env bash
#
# sandbox-setup.sh — scaffold a THROWAWAY sandbox for a live run of the autonomous
# loop (ADR-0019). It does the safe, scriptable parts; the two things only you can
# do — create a fine-grained PAT and provide your Anthropic key — it checks for but
# never handles (the secret values never pass through this script).
#
# It refuses to touch the canonical upstream: the target must be a separate
# throwaway you own.
#
# Usage (run from inside a clone of this repo):
#   scripts/sandbox-setup.sh <owner>/<sandbox-repo>
#
set -euo pipefail

SANDBOX="${1:?usage: sandbox-setup.sh <owner>/<sandbox-repo>}"

# The canonical repo is whatever this clone points at — never scaffold onto it.
# GitHub repo names are case-insensitive, so compare lowercased; otherwise
# `MyOrg/Repo` would slip past the guard and scaffold the real upstream.
UPSTREAM="$(gh repo view --json nameWithOwner --jq .nameWithOwner)"
lower() { printf '%s' "$1" | tr '[:upper:]' '[:lower:]'; }
if [[ "$(lower "$SANDBOX")" == "$(lower "$UPSTREAM")" ]]; then
  echo "refused: '$SANDBOX' is the upstream ($UPSTREAM). Use a throwaway fork." >&2
  exit 1
fi

# The sandbox must already exist (and be yours). Fork it first if needed.
if ! gh repo view "$SANDBOX" >/dev/null 2>&1; then
  cat >&2 <<EOF
refused: sandbox '$SANDBOX' not found. Create a throwaway first, e.g.:
  gh repo fork $UPSTREAM --fork-name <name> --clone=false
EOF
  exit 1
fi

echo "== scaffolding sandbox: $SANDBOX =="

# 1. Flag it as a sandbox — the workflow's live guard refuses without this.
gh variable set BOLT_HARNESS_SANDBOX --body true --repo "$SANDBOX"
echo "  [ok] BOLT_HARNESS_SANDBOX=true"

# 2. Verify the two secrets exist. Set by YOU — this script never sees a value.
missing=0
have_secret() {
  gh secret list --repo "$SANDBOX" --json name --jq '.[].name' 2>/dev/null | grep -qx "$1"
}
for s in BOLT_COSMATIC_BOT_TOKEN ANTHROPIC_API_KEY; do
  if have_secret "$s"; then
    echo "  [ok] secret $s present"
  else
    echo "  [!!] secret $s MISSING — set it yourself (the value never touches this script):" >&2
    echo "         gh secret set $s --repo $SANDBOX" >&2
    missing=1
  fi
done
echo "       BOLT_COSMATIC_BOT_TOKEN = a fine-grained PAT scoped to contents+issues+pull_requests on $SANDBOX only."

# 3. Preflight the workflow + Actions (forks disable both by default).
if ! gh workflow view orchestrator-loop.yml --repo "$SANDBOX" >/dev/null 2>&1; then
  echo "refused: orchestrator-loop.yml not found on $SANDBOX. The sandbox must be a" >&2
  echo "fork of $UPSTREAM that ships the workflow; forks disable workflows by default," >&2
  echo "so enable it under Settings -> Actions first." >&2
  exit 1
fi
echo "  [ok] orchestrator-loop.yml present"
actions_enabled="$(gh api "repos/$SANDBOX/actions/permissions" --jq .enabled 2>/dev/null || echo unknown)"
if [[ "$actions_enabled" != "true" ]]; then
  echo "  [!!] Actions look disabled on $SANDBOX (enabled=$actions_enabled) — turn them" >&2
  echo "       on under Settings -> Actions, or the launch will not run." >&2
fi

# 4. A throwaway issue to drive the loop.
issue_url="$(gh issue create --repo "$SANDBOX" --title "sandbox: live loop smoke" \
  --body "Throwaway issue for an \`bolt-cosmatic loop\` live run. Safe to close.")"
issue_num="${issue_url##*/}"
echo "  [ok] issue #$issue_num created: $issue_url"

# 5. The launch command.
echo "---"
if [[ "$missing" -eq 1 ]]; then
  echo "Set the missing secret(s) above, then launch (the loop merges + deploys"
  echo "autonomously on $SANDBOX — never on the upstream):"
else
  echo "Ready. Launch the live loop (it merges + deploys autonomously on $SANDBOX"
  echo "— never on the upstream):"
fi
echo "  gh workflow run orchestrator-loop.yml --repo $SANDBOX -f issue=$issue_num -f mode=live"
echo "  gh run watch --repo $SANDBOX"
echo ""
echo "When you're done, disarm the sandbox so it can never run live again by accident:"
echo "  gh variable delete BOLT_HARNESS_SANDBOX --repo $SANDBOX"
