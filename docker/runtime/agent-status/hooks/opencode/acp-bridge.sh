#!/bin/sh
# acp-bridge.sh — OpenCode ACP stdio JSON-RPC → jackin status reporter bridge.
#
# Spawns `opencode acp` and translates JSON-RPC notifications to
# jackin agent state reports. Runs as a background process launched
# by the entrypoint when JACKIN_AGENT=opencode.
#
# OpenCode ACP communicates via JSON-RPC 2.0 on stdin/stdout.
# This bridge sends a minimal initialization handshake and then
# processes notification events from the server.

set -eu

REPORTER="/jackin/runtime/agent-status/report.sh"
SESSION_ID="${JACKIN_SESSION_ID:-}"
AGENT="${JACKIN_AGENT_RUNTIME:-opencode}"

if [ -z "$SESSION_ID" ] || [ ! -x "$REPORTER" ]; then
  exit 0
fi

# Verify opencode binary exists.
if ! command -v opencode >/dev/null 2>&1; then
  exit 0
fi

# Verify opencode has acp subcommand.
if ! opencode acp --help >/dev/null 2>&1; then
  exit 0
fi

report() {
  "$REPORTER" --state "$1" --seq "$(date +%s%N 2>/dev/null || date +%s)000" 2>/dev/null || true
}

# Start opencode acp and process its JSON-RPC output.
# opencode acp reads JSON-RPC from stdin and writes notifications to stdout.
# We send an initialize request and then read notification events.
opencode acp 2>/dev/null | python3 -u - <<'PYEOF'
import sys
import json
import os
import subprocess

reporter = "/jackin/runtime/agent-status/report.sh"
session_id = os.environ.get("JACKIN_SESSION_ID", "")

def report(state):
    if session_id and os.path.isfile(reporter) and os.access(reporter, os.X_OK):
        seq = str(int(subprocess.check_output(["date", "+%s%N"]).strip()) if True else 0)
        try:
            subprocess.run([reporter, "--state", state, "--seq", seq], timeout=1)
        except Exception:
            pass

# Event → state mapping for OpenCode ACP notifications
STATE_MAP = {
    "session.idle": "idle",
    "session.busy": "working",
    "question.asked": "blocked",
    "tool.call": "working",
    "agent.start": "working",
    "agent.end": "idle",
}

for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    try:
        msg = json.loads(line)
    except json.JSONDecodeError:
        continue

    method = msg.get("method", "")
    params = msg.get("params", {}) or {}

    # Direct method name mapping
    if method in STATE_MAP:
        report(STATE_MAP[method])
        continue

    # session.status events: check type field
    if method == "session.status":
        status_type = params.get("type") or params.get("status", {}).get("type", "")
        if status_type == "idle":
            report("idle")
        elif status_type in ("busy", "active"):
            if params.get("waitingOnApproval") or params.get("waitingOnUserInput"):
                report("blocked")
            else:
                report("working")
        elif status_type == "question":
            report("blocked")

    # question events
    elif method in ("question.create", "question.ask"):
        report("blocked")

PYEOF
