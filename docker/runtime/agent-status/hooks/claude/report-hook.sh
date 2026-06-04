#!/bin/sh
# report-hook.sh — Claude Code hook reporter for jackin' agent runtime status.
#
# Installed at: /jackin/runtime/agent-status/hooks/claude/report-hook.sh
# Registered in: /home/agent/.claude/settings.json
#
# Reads the hook event from stdin as JSON, maps it to a raw state, and calls
# the jackin reporter. Runs as async for most events; sync for Stop and
# PermissionRequest so we can optionally block.

set -eu

REPORTER="/jackin/runtime/agent-status/report.sh"
SESSION_ID="${JACKIN_SESSION_ID:-}"

# Must have session ID and reporter available.
if [ -z "$SESSION_ID" ] || [ ! -x "$REPORTER" ]; then
  exit 0
fi

# Read hook input from stdin.
HOOK_INPUT="$(cat)"
EVENT="$(printf '%s' "$HOOK_INPUT" | python3 -c "import json,sys; d=json.load(sys.stdin); print(d.get('hook_event_name',''))" 2>/dev/null || echo "")"

if [ -z "$EVENT" ]; then
  exit 0
fi

SEQ="$(date +%s%N 2>/dev/null || date +%s)000"

report() {
  "$REPORTER" --state "$1" --seq "$SEQ" 2>/dev/null || true
}

case "$EVENT" in
  UserPromptSubmit)
    report working
    ;;
  PreToolUse)
    report working
    ;;
  PostToolUse)
    report working
    ;;
  PostToolUseFailure)
    report working
    ;;
  PermissionRequest)
    report blocked
    # PermissionRequest is synchronous — allow Claude to proceed.
    printf '{"continue":true}'
    ;;
  PermissionDenied)
    report working
    ;;
  Stop)
    # Check for running background tasks before reporting idle.
    BACKGROUND_RUNNING=0
    STOP_HOOK_ACTIVE="$(printf '%s' "$HOOK_INPUT" | python3 -c "import json,sys; d=json.load(sys.stdin); print(d.get('stop_hook_active',False))" 2>/dev/null || echo "False")"

    if [ "$STOP_HOOK_ACTIVE" != "True" ] && [ "$STOP_HOOK_ACTIVE" != "true" ]; then
      BACKGROUND_RUNNING="$(printf '%s' "$HOOK_INPUT" | python3 -c "
import json,sys
d=json.load(sys.stdin)
tasks = d.get('background_tasks', [])
running = sum(1 for t in tasks if t.get('status') == 'running')
print(running)
" 2>/dev/null || echo "0")"
    fi

    if [ "$BACKGROUND_RUNNING" -gt "0" ]; then
      report working
      # Block the stop so Claude keeps running.
      printf '{"decision":"block","reason":"background tasks still running"}'
    else
      report idle
    fi
    ;;
  StopFailure)
    report working
    ;;
  TaskCreated)
    report working
    ;;
  TaskCompleted)
    report working
    ;;
  SubagentStart)
    "$REPORTER" --state working --seq "$SEQ" --message SubagentStart 2>/dev/null || true
    ;;
  SubagentStop)
    # SubagentStop must NOT change state — only decrements the subagent counter.
    "$REPORTER" --state working --seq "$SEQ" --message SubagentStop 2>/dev/null || true
    ;;
  Notification)
    NOTIF_TYPE="$(printf '%s' "$HOOK_INPUT" | python3 -c "import json,sys; d=json.load(sys.stdin); print(d.get('notification_type',''))" 2>/dev/null || echo "")"
    case "$NOTIF_TYPE" in
      permission_prompt|elicitation_dialog)
        report blocked
        ;;
    esac
    ;;
  *)
    # Unknown events: no state change.
    ;;
esac
