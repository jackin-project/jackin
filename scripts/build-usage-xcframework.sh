#!/usr/bin/env bash
# Thin wrapper — logic lives in `cargo xtask desktop xcframework`.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
exec cargo xtask desktop xcframework
