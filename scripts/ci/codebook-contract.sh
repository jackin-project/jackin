#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Alexey Zhokhov
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

{
  grep -F '"cargo:codebook-lsp"' mise.toml
  sed -n '/\[\[tools\."cargo:codebook-lsp"\]\]/,/^$/p' mise.lock
} | sha256sum | cut -d ' ' -f 1
