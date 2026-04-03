#!/bin/bash
set -euo pipefail

run_maybe_quiet() {
    if [ "${CLAUDE_DEBUG:-0}" = "1" ]; then
        "$@"
    else
        "$@" > /dev/null 2>&1
    fi
}

# Configure git identity from host environment
if [ -n "${GIT_AUTHOR_NAME:-}" ]; then
    git config --global user.name "$GIT_AUTHOR_NAME"
fi
if [ -n "${GIT_AUTHOR_EMAIL:-}" ]; then
    git config --global user.email "$GIT_AUTHOR_EMAIL"
fi

run_maybe_quiet /home/claude/install-plugins.sh

printf '\033[2J\033[H'

exec env CLAUDE_ENV=docker claude --dangerously-skip-permissions --verbose
