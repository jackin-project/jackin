#!/usr/bin/env bash
# Thin wrapper — logic lives in `cargo xtask desktop verify`.
# Env: JACKIN_APP_VERSION, JACKIN_APP_BUILD (required). RELEASE_MODE=1 for notarized.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
APP="${1:-}"
ZIP="${2:-}"
if [[ -z "$APP" ]]; then
  echo "usage: $0 <JackinDesktop.app> [archive.zip]" >&2
  exit 2
fi
args=(desktop verify "$APP")
if [[ -n "$ZIP" ]]; then
  args+=("$ZIP")
fi
if [[ "${RELEASE_MODE:-0}" == "1" ]]; then
  args+=(--release)
fi
exec cargo xtask "${args[@]}"
