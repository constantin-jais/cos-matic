#!/usr/bin/env bash
#
# Deprecated compatibility entrypoint.
#
# Live sandbox setup is owned by `bolt-harness`, not by the canonical
# `bolt-cos-matic` engine repository. Keeping this script as a refusal avoids
# accidental live setup from the engine checkout while giving existing operators
# a clear migration path.
#
set -euo pipefail

cat >&2 <<'EOF'
refused: live sandbox setup has moved to bolt-harness.

Use:
  git clone https://github.com/constantin-jais/bolt-harness.git
  cd bolt-harness
  scripts/setup-sandbox.sh <owner/bolt-harness-sandbox>

The engine repository only keeps read-only dry-run smoke workflows.
EOF
exit 1
