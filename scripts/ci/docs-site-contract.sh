#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Alexey Zhokhov
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

{
  printf 'docs-site-contract-v1\n'
  git ls-files -z -- \
    'Cargo.toml' \
    'crates/*/Cargo.toml' \
    'crates/*/README.md' \
    'docs/bun.lock' \
    'docs/package.json' \
    'docs/src/**' \
    'docs/content/**' \
    'docs/public/**' \
    'docs/scripts/**' \
    'docs/*.ts' \
    'docs/*.json' \
    'docs/*.toml' \
    | while IFS= read -r -d '' path; do
        printf '%s\0%s\n' "$path" "$(git hash-object "$path")"
      done
  sed -n -e '/^bun = /p' -e '/^node = /p' mise.toml
  sed -n \
    -e '/^\[\[tools\.bun\]\]/,/^\[\[tools\./p' \
    -e '/^\[\[tools\.node\]\]/,/^\[\[tools\./p' \
    mise.lock
} | sha256sum | cut -d ' ' -f 1
