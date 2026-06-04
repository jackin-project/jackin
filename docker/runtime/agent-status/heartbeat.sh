#!/bin/sh
# heartbeat.sh — send periodic HeartbeatAgentAuthority messages to keep
# hook authority alive while the agent is running.
#
# Launched in the background by entrypoint.sh once the agent starts.
# Terminates when the parent process group exits.

set -eu

SOCKET="${JACKIN_STATUS_SOCKET:-/jackin/run/jackin.sock}"
SESSION_ID="${JACKIN_SESSION_ID:-}"
SOURCE_ID="${JACKIN_STATUS_SOURCE:-reporter}"
INTERVAL="${JACKIN_HEARTBEAT_INTERVAL:-10}"

if [ -z "$SESSION_ID" ]; then
  exit 0
fi

while true; do
  sleep "$INTERVAL"
  SEQ="$(date +%s%N 2>/dev/null || date +%s)000"
  JSON="{\"type\":\"heartbeat_agent_authority\",\"session_id\":$SESSION_ID,\"source_id\":\"$SOURCE_ID\",\"seq\":$SEQ}"

  if command -v python3 >/dev/null 2>&1; then
    printf '%s' "$JSON" | python3 -c "
import sys, struct
data = sys.stdin.buffer.read()
header = struct.pack('>I', len(data))
sys.stdout.buffer.write(header + data)
" | nc -U "$SOCKET" >/dev/null 2>&1 || true
  fi
done
