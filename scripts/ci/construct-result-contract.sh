#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Alexey Zhokhov
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

{
  printf 'construct-result-contract-v1\n'
  git ls-files -z -- \
    '.cargo/**' \
    'Cargo.lock' \
    'Cargo.toml' \
    'crates/*/Cargo.toml' \
    'crates/jackin-process/src/**' \
    'crates/jackin-xtask/src/cmd.rs' \
    'crates/jackin-xtask/src/construct.rs' \
    'crates/jackin-xtask/src/construct/**' \
    'crates/jackin-xtask/src/main.rs' \
    'docker-bake.hcl' \
    'docker/construct/**' \
    'rust-toolchain.toml' \
    | while IFS= read -r -d '' path; do
        printf '%s\0%s\n' "$path" "$(git hash-object "$path")"
      done
} | sha256sum | cut -d ' ' -f 1
