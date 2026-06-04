#!/bin/sh
# report-hook.sh — Kimi hook reporter for jackin' agent runtime status.
# Hook format requires verification against the installed Kimi CLI version.
# This is a placeholder for Phase 3 full implementation.
#
# Installed at: /jackin/runtime/agent-status/hooks/kimi/report-hook.sh

REPORTER="/jackin/runtime/agent-status/report.sh"
SESSION_ID="${JACKIN_SESSION_ID:-}"

if [ -z "$SESSION_ID" ] || [ ! -x "$REPORTER" ]; then
  exit 0
fi

HOOK_INPUT="$(cat 2>/dev/null || echo "{}")"
EVENT="$(printf '%s' "$HOOK_INPUT" | python3 -c "import json,sys; d=json.load(sys.stdin); print(d.get('hook_event_name',''))" 2>/dev/null || echo "")"
SEQ="$(date +%s%N 2>/dev/null || date +%s)000"

case "$EVENT" in
  UserPromptSubmit|PreToolUse|PostToolUse) "$REPORTER" --state working --seq "$SEQ" 2>/dev/null || true ;;
  PermissionRequest) "$REPORTER" --state blocked --seq "$SEQ" 2>/dev/null || true ;;
  Stop|StopFailure) "$REPORTER" --state idle --seq "$SEQ" 2>/dev/null || true ;;
esac
