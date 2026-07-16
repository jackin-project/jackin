#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Alexey Zhokhov
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

# Docs consumes only the documentation command surface of jackin-xtask. Keep
# unrelated command implementations out of this contract so their changes do
# not force Docs to wait for a redundant binary rebuild.
git ls-files -z -- \
  '.cargo/**' \
  'Cargo.lock' \
  'Cargo.toml' \
  'rust-toolchain.toml' \
  'crates/*/Cargo.toml' \
  'crates/jackin-xtask/src/arch.rs' \
  'crates/jackin-xtask/src/cmd.rs' \
  'crates/jackin-xtask/src/docs.rs' \
  'crates/jackin-xtask/src/docs/**' \
  'crates/jackin-xtask/src/fs_util.rs' \
  'crates/jackin-xtask/src/main.rs' \
  'crates/jackin-xtask/src/report.rs' \
  | while IFS= read -r -d '' path; do
      printf '%s\0%s\n' "$path" "$(git hash-object "$path")"
    done \
  | sha256sum \
  | cut -d ' ' -f 1
