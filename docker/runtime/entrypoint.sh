#!/bin/bash
set -euo pipefail

run_maybe_quiet() {
    if [ "${CLAUDE_DEBUG:-0}" = "1" ]; then
        "$@"
    else
        "$@" > /dev/null 2>&1
    fi
}

run_maybe_quiet /home/claude/install-plugins.sh

printf '\033[2J\033[H'

exec env CLAUDE_ENV=docker claude --dangerously-skip-permissions --verbose
