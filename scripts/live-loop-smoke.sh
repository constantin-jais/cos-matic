#!/usr/bin/env bash
#
# Dry-run smoke test for the autonomous loop wiring in the engine repository.
#
# Live sandbox runs belong in `bolt-harness` so the canonical engine repository
# does not expose an active write-capable workflow or local live helper.
#
# Usage:
#   export GITHUB_TOKEN="$(gh auth token)" # optional when the dry-run needs GitHub API access
#   scripts/live-loop-smoke.sh <issue-number> [title]
#
set -euo pipefail

ISSUE="${1:?usage: live-loop-smoke.sh <issue-number> [title]}"
TITLE="${2:-dry-run smoke test}"

cd "$(git rev-parse --show-toplevel)"

echo "== dry-run (safe, read-only) =="
cargo run -q --bin bolt-cosmatic -- loop --dry-run --issue "$ISSUE" --title "$TITLE"

cat <<'EOF'

Live sandbox execution has moved to bolt-harness:
  https://github.com/constantin-jais/bolt-harness

Use bolt-harness/.github/workflows/live-sandbox.yml or its setup script for
write-capable sandbox demonstrations.
EOF
