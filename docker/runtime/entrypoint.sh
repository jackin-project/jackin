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

# ── runtime-neutral setup ──────────────────────────────────────────
# Configure git identity from host environment
if [ -n "${GIT_AUTHOR_NAME:-}" ]; then
    git config --global user.name "$GIT_AUTHOR_NAME"
fi
if [ -n "${GIT_AUTHOR_EMAIL:-}" ]; then
    git config --global user.email "$GIT_AUTHOR_EMAIL"
fi

# Authenticate with GitHub if gh is installed in the container
if [ -x /usr/bin/gh ]; then
    if gh auth status &>/dev/null; then
        echo "[entrypoint] GitHub CLI already authenticated"
        gh auth setup-git
        git config --global url."https://github.com/".insteadOf "git@github.com:"
    else
        echo "[entrypoint] GitHub CLI not authenticated — skipping login (run 'gh auth login' inside the runtime if needed)"
    fi
else
    echo "[entrypoint] GitHub CLI not installed — skipping auth"
fi

# ── harness-specific setup ─────────────────────────────────────────
case "${JACKIN_HARNESS:?JACKIN_HARNESS must be set}" in
  claude)
    run_maybe_quiet /home/agent/install-claude-plugins.sh

    # Register security tool MCP servers (ignore "already exists" on subsequent runs)
    if [[ "${JACKIN_DISABLE_TIRITH:-0}" != "1" ]]; then
        run_maybe_quiet claude mcp add tirith -- tirith mcp-server || true
    else
        echo "[entrypoint] tirith disabled (JACKIN_DISABLE_TIRITH=1)"
    fi
    if [[ "${JACKIN_DISABLE_SHELLFIRM:-0}" != "1" ]]; then
        run_maybe_quiet claude mcp add shellfirm -- shellfirm mcp || true
    else
        echo "[entrypoint] shellfirm disabled (JACKIN_DISABLE_SHELLFIRM=1)"
    fi

    LAUNCH=(claude --dangerously-skip-permissions --verbose)
    ;;
  codex)
    # config.toml is mounted RW from host; no in-container generation needed.
    LAUNCH=(codex)
    ;;
  *)
    echo "[entrypoint] unknown JACKIN_HARNESS: $JACKIN_HARNESS" >&2
    exit 2
    ;;
esac

# ── pre-launch hook (runtime-neutral) ──────────────────────────────
if [ -x /home/agent/.jackin-runtime/pre-launch.sh ]; then
    echo "Running pre-launch hook..."
    /home/agent/.jackin-runtime/pre-launch.sh
fi

# In debug mode, pause so the operator can review logs before the harness clears the screen
if [ "${JACKIN_DEBUG:-0}" = "1" ]; then
    set +x
    echo ""
    echo "[entrypoint] Setup complete. Press Enter to launch ${JACKIN_HARNESS}..."
    read -r
    set -x
fi

printf '\033[2J\033[H'

exec "${LAUNCH[@]}"
