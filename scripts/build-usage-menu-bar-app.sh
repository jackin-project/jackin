#!/usr/bin/env bash
# Thin wrapper — logic lives in `cargo xtask desktop build`.
# Requires JACKIN_APP_VERSION and JACKIN_APP_BUILD (or pass via cargo xtask flags).
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
exec cargo xtask desktop build
