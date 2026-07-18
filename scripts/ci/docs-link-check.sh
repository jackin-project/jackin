#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Alexey Zhokhov
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

: "${DOCS_SITE_URL:?DOCS_SITE_URL is required}"
: "${GITHUB_WORKSPACE:?GITHUB_WORKSPACE is required}"
: "${JACKIN_REPO_BLOB_URL:?JACKIN_REPO_BLOB_URL is required}"
: "${JACKIN_REPO_EDIT_URL:?JACKIN_REPO_EDIT_URL is required}"

MISE_CONFIG_FILE=docs/mise.toml mise exec -- lychee \
  --config docs/lychee.toml \
  --include-fragments \
  --remap "${DOCS_SITE_URL}/(.*) file://${GITHUB_WORKSPACE}/docs/.output/public/\$1" \
  --remap "${JACKIN_REPO_EDIT_URL}/(.*) file://${GITHUB_WORKSPACE}/\$1" \
  --remap "${JACKIN_REPO_BLOB_URL}/(.*) file://${GITHUB_WORKSPACE}/\$1" \
  --remap "https://github.com/jackin-project/jackin/issues https://api.github.com/repos/jackin-project/jackin/issues" \
  --root-dir "${GITHUB_WORKSPACE}/docs/.output/public" \
  "docs/.output/public/**/*.html"
