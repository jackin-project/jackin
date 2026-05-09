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

# Run unconditionally — works whether gh is authed or not. Container
# git push needs HTTPS and the gh credential helper, even on first
# launch when gh has nothing to forward yet. Both `git@github.com:`
# (SCP-style) and `ssh://git@github.com/` forms are caught so a repo
# the operator originally cloned via SSH on the host can still be
# pushed from inside the container without an SSH key.
# Use --add so both insteadOf patterns coexist on the same key. Plain
# `git config <key> <value>` is single-valued — the second invocation
# would overwrite the first and silently drop one of the two forms.
git config --global --add url."https://github.com/".insteadOf "git@github.com:"
git config --global --add url."https://github.com/".insteadOf "ssh://git@github.com/"

if [ -x /usr/bin/gh ]; then
    # Credential helper resolves to either GH_TOKEN env (preferred) or
    # the configured hosts.yml. Either way, git push/fetch over HTTPS
    # uses the gh-resolved token without prompting.
    git config --global credential.helper '!gh auth git-credential'

    if [ -n "${GH_TOKEN:-}" ] || gh auth status &>/dev/null; then
        echo "[entrypoint] GitHub CLI authenticated (host: github.com)"
        gh auth setup-git
    else
        echo "[entrypoint] GitHub CLI not authenticated — run 'gh auth login' inside the runtime if needed"
    fi
else
    echo "[entrypoint] GitHub CLI not installed — skipping gh setup"
fi

# ── agent-specific setup ───────────────────────────────────────────
#
# Auth/config files arrive under /jackin/<agent>/... rather than being
# bind-mounted directly over the agent's home. The image bakes
# ~/.claude/{settings.json,hooks,memory} (and the codex equivalents);
# bind-mounting on top would mask those, so we copy from /jackin/ into
# the agent home here at startup. Copies — not symlinks — to avoid
# tools that resolve realpath and refuse paths outside $HOME, and so
# in-session writes (token rotation, etc.) stay in the container's
# writable layer instead of leaking back to the host.
case "${JACKIN_AGENT:?JACKIN_AGENT must be set}" in
  claude)
    mkdir -p /home/agent/.claude
    if [ -f /jackin/claude/account.json ]; then
        cp /jackin/claude/account.json /home/agent/.claude.json
        chmod 600 /home/agent/.claude.json
    fi
    if [ -f /jackin/claude/credentials.json ]; then
        cp /jackin/claude/credentials.json /home/agent/.claude/.credentials.json
        chmod 600 /home/agent/.claude/.credentials.json
    fi

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
    mkdir -p /home/agent/.codex
    if [ -f /jackin/codex/config.toml ]; then
        cp /jackin/codex/config.toml /home/agent/.codex/config.toml
        chmod 600 /home/agent/.codex/config.toml
    fi
    if [ -f /jackin/codex/auth.json ]; then
        cp /jackin/codex/auth.json /home/agent/.codex/auth.json
        chmod 600 /home/agent/.codex/auth.json
    fi
    LAUNCH=(codex)
    ;;
  amp)
    mkdir -p /home/agent/.local/share/amp
    if [ -f /jackin/amp/secrets.json ]; then
        echo "[entrypoint] amp: forwarding host secrets.json into ~/.local/share/amp/" >&2
        cp /jackin/amp/secrets.json /home/agent/.local/share/amp/secrets.json
        chmod 600 /home/agent/.local/share/amp/secrets.json
    elif [ -n "${AMP_API_KEY:-}" ]; then
        echo "[entrypoint] amp: AMP_API_KEY present in env; agent will use api-key auth" >&2
    else
        echo "[entrypoint] amp: no secrets.json mounted and AMP_API_KEY unset — agent will require interactive login" >&2
    fi
    # Mirrors Claude's `--dangerously-skip-permissions`. CLI flag chosen
    # over `amp.dangerouslyAllowAll: true` so jackin doesn't write to the
    # operator's XDG_CONFIG.
    LAUNCH=(amp --dangerously-allow-all)
    ;;
  *)
    echo "[entrypoint] unknown JACKIN_AGENT: $JACKIN_AGENT" >&2
    exit 2
    ;;
esac

# ── pre-launch hook (runtime-neutral) ──────────────────────────────
if [ -x /home/agent/.jackin-runtime/pre-launch.sh ]; then
    echo "Running pre-launch hook..."
    /home/agent/.jackin-runtime/pre-launch.sh
fi

# In debug mode, pause so the operator can review logs before the agent clears the screen
if [ "${JACKIN_DEBUG:-0}" = "1" ]; then
    set +x
    echo ""
    echo "[entrypoint] Setup complete. Press Enter to launch ${JACKIN_AGENT}..."
    read -r
    set -x
fi

printf '\033[2J\033[H'

exec "${LAUNCH[@]}"
