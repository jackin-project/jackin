#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Alexey Zhokhov
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

ref=${1:-HEAD}

{
  printf 'docs-link-contract-v1\n'
  scripts/ci/docs-site-contract.sh "$ref"
  scripts/ci/docs-lychee-contract.sh "$ref"
  git rev-parse "$ref:scripts/ci/docs-link-check.sh"
} | sha256sum | cut -d ' ' -f 1
