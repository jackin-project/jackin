#!/usr/bin/env bash
# Thin wrapper — logic lives in `cargo xtask desktop bindings`.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
PROFILE="${PROFILE:-release}"
exec cargo xtask desktop bindings --profile "$PROFILE"
