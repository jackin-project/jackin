#!/bin/sh
# report.sh — send a ReportAgentState control message to the Capsule socket.
#
# Usage: report.sh --state working|blocked|idle|unknown --seq <ns_timestamp>
#        [--message <text>] [--session <session_id>] [--source <source_id>]
#
# Environment variables (set by the container at agent launch):
#   JACKIN_SESSION_ID    — stable session ID for the lifetime of this session.
#   JACKIN_STATUS_SOCKET — path to the Capsule Unix socket.
#   JACKIN_STATUS_SOURCE — source ID for this session's reporter.
#   JACKIN_AGENT_RUNTIME — agent slug (claude, codex, amp, kimi, opencode).

set -euo pipefail

SOCKET="${JACKIN_STATUS_SOCKET:-/jackin/run/jackin.sock}"
SESSION_ID="${JACKIN_SESSION_ID:-}"
SOURCE_ID="${JACKIN_STATUS_SOURCE:-reporter}"
AGENT_RUNTIME="${JACKIN_AGENT_RUNTIME:-unknown}"

STATE=""
SEQ=""
MESSAGE=""

while [ "$#" -gt 0 ]; do
  case "$1" in
    --state)   STATE="$2";   shift 2 ;;
    --seq)     SEQ="$2";     shift 2 ;;
    --message) MESSAGE="$2"; shift 2 ;;
    --session) SESSION_ID="$2"; shift 2 ;;
    --source)  SOURCE_ID="$2";  shift 2 ;;
    *) shift ;;
  esac
done

if [ -z "$STATE" ] || [ -z "$SEQ" ] || [ -z "$SESSION_ID" ]; then
  echo "report.sh: --state, --seq, and JACKIN_SESSION_ID are required" >&2
  exit 1
fi

# Validate state value.
case "$STATE" in
  working|blocked|idle|unknown) ;;
  *) echo "report.sh: invalid state '$STATE'" >&2; exit 1 ;;
esac

# Build the JSON payload. The Capsule socket expects a 4-byte big-endian
# length prefix followed by a UTF-8 JSON body (same framing as control.rs::frame).
TS_NS="$(date +%s%N 2>/dev/null || echo "$SEQ")"

if [ -n "$MESSAGE" ]; then
  # Escape the message for JSON embedding.
  ESCAPED_MSG="$(printf '%s' "$MESSAGE" | sed 's/\\/\\\\/g; s/"/\\"/g; s/	/\\t/g')"
  JSON="{\"type\":\"report_agent_state\",\"session_id\":$SESSION_ID,\"source_id\":\"$SOURCE_ID\",\"agent_label\":\"$AGENT_RUNTIME\",\"raw_state\":\"$STATE\",\"seq\":$SEQ,\"ts_ns\":$TS_NS,\"message\":\"$ESCAPED_MSG\"}"
else
  JSON="{\"type\":\"report_agent_state\",\"session_id\":$SESSION_ID,\"source_id\":\"$SOURCE_ID\",\"agent_label\":\"$AGENT_RUNTIME\",\"raw_state\":\"$STATE\",\"seq\":$SEQ,\"ts_ns\":$TS_NS}"
fi

# Build a binary length prefix (4 bytes big-endian) and send.
if command -v python3 >/dev/null 2>&1; then
  printf '%s' "$JSON" | python3 -c "
import sys, struct
data = sys.stdin.buffer.read()
header = struct.pack('>I', len(data))
sys.stdout.buffer.write(header + data)
" | nc -U "$SOCKET" >/dev/null 2>&1 || echo "report.sh: socket write failed (socket=$SOCKET)" >&2
elif command -v python >/dev/null 2>&1; then
  printf '%s' "$JSON" | python -c "
import sys, struct
data = sys.stdin.read()
header = struct.pack('>I', len(data))
sys.stdout.write(header + data)
" | nc -U "$SOCKET" >/dev/null 2>&1 || echo "report.sh: socket write failed (socket=$SOCKET)" >&2
else
  echo "report.sh: python3/python not available; cannot report state to $SOCKET" >&2
fi
