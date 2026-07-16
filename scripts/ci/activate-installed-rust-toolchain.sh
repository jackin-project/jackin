#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Alexey Zhokhov
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

version=$1
if toolchain=$(scripts/ci/find-installed-rust-toolchain.sh "$version" 2>/dev/null); then
  echo "RUSTUP_TOOLCHAIN=$toolchain" >> "${GITHUB_ENV:?GITHUB_ENV must be set}"
  exit 0
fi

mise_install=${MISE_DATA_DIR:?MISE_DATA_DIR must be set}/installs/rust/$version
if [ -x "$mise_install/bin/rustc" ] && [ -x "$mise_install/bin/cargo" ]; then
  echo "$mise_install/bin" >> "${GITHUB_PATH:?GITHUB_PATH must be set}"
  exit 0
fi

echo "prepared Rust toolchain $version is unavailable in rustup or mise storage" >&2
exit 1
