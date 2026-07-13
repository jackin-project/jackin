#!/bin/sh

# SPDX-FileCopyrightText: 2026 Alexey Zhokhov
# SPDX-License-Identifier: Apache-2.0

set -eu

CAPSULE="/jackin/runtime/jackin-capsule"

if [ -z "${JACKIN_SESSION_ID:-}" ] || [ ! -x "$CAPSULE" ]; then
  exit 0
fi

"$CAPSULE" report-event "$@" --payload-stdin 2>/dev/null || true
exit 0
