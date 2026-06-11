#!/bin/sh
set -eu

CAPSULE="/jackin/runtime/jackin-capsule"

if [ -z "${JACKIN_SESSION_ID:-}" ] || [ ! -x "$CAPSULE" ]; then
  exit 0
fi

"$CAPSULE" report-event "$@" --payload-stdin 2>/dev/null || true
exit 0
