#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Alexey Zhokhov
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

ref=${1:-HEAD}

{
  printf 'docs-site-contract-v2\n'
  git ls-tree -r -z "$ref" \
    | while IFS= read -r -d '' entry; do
        metadata=${entry%%$'\t'*}
        path=${entry#*$'\t'}
        case "$path" in
          Cargo.toml | \
          crates/*/Cargo.toml | \
          crates/*/README.md | \
          docs/bun.lock | \
          docs/package.json | \
          docs/src/* | \
          docs/content/* | \
          docs/public/* | \
          docs/scripts/* | \
          docs/*.ts | \
          docs/*.json)
            printf '%s\0%s\n' "$path" "${metadata##* }"
            ;;
        esac
      done
  git show "$ref:mise.toml" | sed -n -e '/^bun = /p' -e '/^node = /p'
  git show "$ref:mise.lock" | sed -n \
    -e '/^\[\[tools\.bun\]\]/,/^\[\[tools\./p' \
    -e '/^\[\[tools\.node\]\]/,/^\[\[tools\./p'
} | sha256sum | cut -d ' ' -f 1
