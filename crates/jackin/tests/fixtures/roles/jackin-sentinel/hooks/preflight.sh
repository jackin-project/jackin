#!/usr/bin/env sh

# SPDX-FileCopyrightText: 2026 Alexey Zhokhov
# SPDX-License-Identifier: Apache-2.0

set -eu

state_dir="${JACKIN_SENTINEL_STATE_DIR:-/jackin/state/jackin-sentinel}"
mkdir -p "$state_dir"

if [ "${JACKIN_SENTINEL_SOURCE_HOOK:-}" != "1" ]; then
  echo "source hook did not run before preflight" >&2
  exit 42
fi

{
  printf 'preflight=1\n'
  printf 'agent=%s\n' "${JACKIN_AGENT:-unset}"
  printf 'select_project=%s\n' "${SELECT_PROJECT:-unset}"
  printf 'select_mode=%s\n' "${SELECT_MODE:-unset}"
  printf 'branch=%s\n' "${BRANCH:-unset}"
  printf 'combined_label=%s\n' "${COMBINED_LABEL:-unset}"
} >> "$state_dir/preflight.log"
