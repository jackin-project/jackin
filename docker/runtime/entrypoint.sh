#!/bin/bash

# SPDX-FileCopyrightText: 2026 Alexey Zhokhov
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

run_hook() {
    local label="$1" path="$2" tail="${3:-}" hook_cwd="${4:-}"
    echo "[entrypoint] running $label hook..."
    local rc=0
    if [ -n "$hook_cwd" ]; then
        ( cd "$hook_cwd" && "$path" ) || rc=$?
    else
        "$path" || rc=$?
    fi
    if [ "$rc" -ne 0 ]; then
        echo "[entrypoint] $label hook failed (exit $rc)${tail:+; $tail}" >&2
        exit "$rc"
    fi
}

/jackin/runtime/jackin-capsule runtime-setup

export JACKIN_HOOK_STATE_DIR="${JACKIN_SESSION_STATE_DIR:-/jackin/state}/hook-state"
if ! mkdir -p "$JACKIN_HOOK_STATE_DIR"; then
    echo "[entrypoint] failed to create hook state directory $JACKIN_HOOK_STATE_DIR" >&2
    exit 1
fi

export JACKIN_STATUS_SOCKET="${JACKIN_STATUS_SOCKET:-/jackin/run/jackin.sock}"
export JACKIN_STATUS_SOURCE="${JACKIN_STATUS_SOURCE:-wrapper-${JACKIN_SESSION_ID:-0}}"
export JACKIN_AGENT_RUNTIME="${JACKIN_AGENT:-unknown}"











JACKIN_EXEC_SYSTEM_PROMPT=""
if [ -n "${JACKIN_EXEC_BINDINGS:-}" ]; then
    JACKIN_EXEC_SYSTEM_PROMPT="$(cat <<'EXEC_PROMPT'
When you need to execute commands that require credentials — SSH connections,
registry logins, cloud CLI commands (aws, kubectl, gh), git push — call
jackin-exec <command> instead of running the command directly. jackin' will
securely inject the required credentials. Available secure bindings:
EXEC_PROMPT
)"
    JACKIN_EXEC_SYSTEM_PROMPT="${JACKIN_EXEC_SYSTEM_PROMPT}
${JACKIN_EXEC_BINDINGS}"
fi

case "${JACKIN_AGENT:?JACKIN_AGENT must be set}" in
  claude)
    LAUNCH=(claude --settings '{"skipDangerousModePermissionPrompt":true}' --dangerously-skip-permissions --verbose)
    if [ -n "${JACKIN_EXEC_SYSTEM_PROMPT:-}" ]; then
        LAUNCH+=(--system-prompt "${JACKIN_EXEC_SYSTEM_PROMPT}")
    fi
    if [ "$#" -gt 0 ]; then
        LAUNCH+=("$@")
    fi
    ;;
  codex)
    LAUNCH=(codex --enable goals --dangerously-bypass-approvals-and-sandbox)
    if [ "$#" -gt 0 ]; then
        LAUNCH+=("$@")
    fi
    ;;
  amp)
    LAUNCH=(amp --dangerously-allow-all)
    ;;
  kimi)
    LAUNCH=(kimi --yolo)
    if [ "$#" -gt 0 ]; then
        LAUNCH+=("$@")
    fi
    ;;
  opencode)
    export OPENCODE_CONFIG_CONTENT='{"permission":"allow"}'
    LAUNCH=(opencode)
    if [ $# -gt 0 ]; then
        LAUNCH+=("$@")
    fi
    ;;
  grok)
    LAUNCH=(grok --always-approve)
    if [ "$#" -gt 0 ]; then
        LAUNCH+=("$@")
    fi
    ;;
  *)
    echo "[entrypoint] unknown JACKIN_AGENT: $JACKIN_AGENT" >&2
    exit 2
    ;;
esac

if [ -x /jackin/runtime/hooks/setup-once.sh ]; then
    setup_once_marker="${JACKIN_SESSION_STATE_DIR:-/jackin/state}/hooks/setup-once.done"
    if [ ! -e "$setup_once_marker" ]; then
        if ! mkdir -p "$(dirname "$setup_once_marker")"; then
            echo "[entrypoint] failed to create marker directory $(dirname "$setup_once_marker")" >&2
            exit 1
        fi
        run_hook setup-once /jackin/runtime/hooks/setup-once.sh \
            "marker not written, will retry next launch"
        if ! touch "$setup_once_marker"; then
            echo "[entrypoint] setup-once succeeded but marker write failed; will retry next launch" >&2
            exit 1
        fi
    fi
fi

if [ -x /jackin/runtime/hooks/source.sh ]; then
    echo "[entrypoint] sourcing runtime hook..."
    source_pwd="$PWD"
    case $- in *x*) source_xtrace=1; set +x ;; esac
    rc=0
    . /jackin/runtime/hooks/source.sh || rc=$?
    if [ "$rc" -ne 0 ]; then
        echo "[entrypoint] source hook returned non-zero (exit $rc); aborting before agent launch" >&2
        exit "$rc"
    fi
    [ "${source_xtrace:-0}" = "1" ] && set -x
    trap - ERR
    if ! cd "$source_pwd"; then
        echo "[entrypoint] saved PWD ($source_pwd) vanished after source hook; cannot launch agent" >&2
        exit 1
    fi
fi

if [ -x /jackin/runtime/hooks/preflight.sh ]; then
    run_hook preflight /jackin/runtime/hooks/preflight.sh "" "$HOME"
fi

exec "${LAUNCH[@]}"
