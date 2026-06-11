#!/bin/sh
# Claude Code hook reporter for jackin' agent runtime status.
#
# Keep this as a dumb pipe: Claude sends the hook payload on stdin, and the
# capsule daemon owns event mapping/gating.

set -eu

CAPSULE="/jackin/runtime/jackin-capsule"

if [ -z "${JACKIN_SESSION_ID:-}" ] || [ ! -x "$CAPSULE" ]; then
  exit 0
fi

"$CAPSULE" report-event "$@" --payload-stdin 2>/dev/null || true

# PermissionRequest is synchronous in Claude Code. Observability must not block
# the agent; it only acknowledges that the hook completed.
printf '{"continue":true}'
