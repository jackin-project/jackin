#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Alexey Zhokhov
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

ref=${1:-HEAD}

{
  git show "$ref:docs/lychee.toml" \
    | python3 -c 'import json, sys, tomllib; json.dump(tomllib.load(sys.stdin.buffer), sys.stdout, sort_keys=True, separators=(",", ":"))'
  git show "$ref:mise.toml" | sed -n -e '/^lychee = /p'
  git show "$ref:mise.lock" | sed -n \
    -e '/^\[\[tools\.lychee\]\]/,/^\[\[tools\./p'
} | sha256sum | cut -d ' ' -f 1
