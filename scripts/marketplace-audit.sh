#!/usr/bin/env bash
# Fail commit if jackin.role.toml declares a plugin from a marketplace
# outside the allow-list. Wired to .pre-commit-config.yaml.
set -euo pipefail
ALLOW='@(claude-plugins-official|tailrocks-skills|caveman)'
bad=$(grep -E '"[^@]+@[^"]+"' jackin.role.toml | grep -Ev "$ALLOW" || true)
if [[ -n "$bad" ]]; then
  echo "Plugin from undocumented marketplace:" >&2
  echo "$bad" >&2
  echo "Update the allow-list in scripts/marketplace-audit.sh and document trust rationale in PR." >&2
  exit 1
fi
