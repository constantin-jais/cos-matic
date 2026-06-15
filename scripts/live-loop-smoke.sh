#!/usr/bin/env bash
#
# Live smoke test for the autonomous loop: dispatch -> publish -> automerge -> deploy.
#
# SAFE BY DEFAULT — runs `cosmatic loop --dry-run` only (read-only: resolves the repo
# and queries the real merge gate, touches nothing). The REAL run is outward and
# irreversible: it dispatches a fixer agent, pushes a branch, opens AND merges a
# PR, then deploys. It therefore requires an explicit opt-in, cosmatic_LIVE=1.
#
# Credentials are NEVER set here — you export your own GITHUB_TOKEN. Run the real
# path only against a THROWAWAY issue on a SANDBOX repo you control.
#
# Usage:
#   export GITHUB_TOKEN="$(gh auth token)"
#   scripts/live-loop-smoke.sh <issue-number> [title]              # dry-run (safe)
#   cosmatic_LIVE=1 scripts/live-loop-smoke.sh <issue-number> [title]   # real run (sandbox!)
#
set -euo pipefail

ISSUE="${1:?usage: live-loop-smoke.sh <issue-number> [title]}"
TITLE="${2:-live smoke test}"

# Precondition: checked, never set for you. The token stays in your shell.
: "${GITHUB_TOKEN:?export GITHUB_TOKEN=\"\$(gh auth token)\" first}"

# No-op deploy: each stage exits 0 so the deploy wiring is exercised end-to-end
# without touching real infrastructure (canary "up", smoke "passes", promote "ok").
export cosmatic_DEPLOY_CANARY="true"
export cosmatic_DEPLOY_PROMOTE="true"
export cosmatic_DEPLOY_ROLLBACK="true"
export cosmatic_DEPLOY_SMOKE="true"

cd "$(git rev-parse --show-toplevel)"

echo "== dry-run (safe, read-only) =="
cargo run -q --bin aom -- loop --dry-run --issue "$ISSUE" --title "$TITLE"

if [[ "${cosmatic_LIVE:-0}" != "1" ]]; then
  cat <<EOF

Dry-run only. The real run dispatches a fixer, pushes, opens+merges a PR, and deploys.
To run it against a THROWAWAY issue on a SANDBOX repo:
  cosmatic_LIVE=1 $0 $ISSUE "$TITLE"
EOF
  exit 0
fi

echo "== LIVE run (outward, irreversible — sandbox only) =="
cargo run -q --bin aom -- loop --issue "$ISSUE" --title "$TITLE"
