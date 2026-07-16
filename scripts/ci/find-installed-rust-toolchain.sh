#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Alexey Zhokhov
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

version=$1
case "$(uname -m)" in
  x86_64) host=x86_64-unknown-linux-gnu ;;
  aarch64|arm64) host=aarch64-unknown-linux-gnu ;;
  *) echo "unsupported Rust host architecture: $(uname -m)" >&2; exit 1 ;;
esac

rustup_home=${RUSTUP_HOME:-${HOME:?HOME must be set}/.rustup}
toolchain=$(find "$rustup_home/toolchains" -mindepth 1 -maxdepth 1 -type d \
  -name "${version}*-${host}" -printf '%f\n' | sort -V | tail -n 1)
if [ -z "$toolchain" ] || [ ! -x "$rustup_home/toolchains/$toolchain/bin/rustc" ]; then
  echo "prepared Rust toolchain ${version}-${host} is unavailable" >&2
  exit 1
fi
printf '%s\n' "$toolchain"
