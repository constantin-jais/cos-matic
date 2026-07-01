#!/usr/bin/env bash
set -euo pipefail

# Canonical dependency audit gate.
#
# cargo-audit does not read deny.toml, so the temporary RustSec exceptions are
# duplicated here and in deny.toml. Keep both lists in sync and remove entries as
# soon as upstream fixes are available.

ignored_advisories=(
  # Transitive via octocrab -> jsonwebtoken -> rsa; bolt-cosmatic does not use RSA private
  # keys directly, and no fixed upstream version is available yet.
  RUSTSEC-2023-0071
  # Transitive via inquire; local interactive prompts only, not a runtime trust
  # boundary. Track upstream/replacement before v0.1 stable.
  RUSTSEC-2025-0057
)

audit_args=(--deny warnings)
for advisory in "${ignored_advisories[@]}"; do
  audit_args+=(--ignore "$advisory")
done

cargo audit "${audit_args[@]}"
cargo deny check advisories licenses sources
