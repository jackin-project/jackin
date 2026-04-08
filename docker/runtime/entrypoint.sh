#!/bin/bash
set -euo pipefail

# Trace all commands in debug mode
if [ "${JACKIN_DEBUG:-0}" = "1" ]; then
    set -x
fi

run_maybe_quiet() {
    if [ "${JACKIN_DEBUG:-0}" = "1" ]; then
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

# Authenticate with GitHub if gh is installed in the container
if [ -x /usr/bin/gh ]; then
    if ! gh auth status &>/dev/null; then
        gh auth login
    fi
    gh auth setup-git
    git config --global url."https://github.com/".insteadOf "git@github.com:"
fi

run_maybe_quiet /home/claude/install-plugins.sh

# Register security tool MCP servers
if [[ "${JACKIN_DISABLE_TIRITH:-0}" != "1" ]]; then
    run_maybe_quiet claude mcp add tirith -- tirith mcp-server
fi
if [[ "${JACKIN_DISABLE_SHELLFIRM:-0}" != "1" ]]; then
    run_maybe_quiet claude mcp add shellfirm -- shellfirm mcp-server
fi

# Run pre-launch hook if present
if [ -x /home/claude/.jackin-runtime/pre-launch.sh ]; then
    echo "Running pre-launch hook..."
    /home/claude/.jackin-runtime/pre-launch.sh
fi

printf '\033[2J\033[H'

exec claude --dangerously-skip-permissions --verbose
