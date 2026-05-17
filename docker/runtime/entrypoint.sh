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

seed_home_dir() {
    local src="$1" dst="$2"
    mkdir -p "$dst"
    if [ -d "$src" ]; then
        cp -an "$src"/. "$dst"/ 2>/dev/null || true
    fi
}

# Run a child-process hook with a `[entrypoint]` log prefix and an
# explicit failure-attributed exit. `$3` (optional) is appended after
# `; ` to the failure line — used by setup-once to surface its retry
# semantics.
run_hook() {
    local label="$1" path="$2" tail="${3:-}"
    echo "[entrypoint] running $label hook..."
    # `$?` inside `if ! cmd; then ...` is the negated test's status (0),
    # not the hook's. Capture before the test so the failure log + exit
    # surface the real exit code.
    local rc=0
    "$path" || rc=$?
    if [ "$rc" -ne 0 ]; then
        echo "[entrypoint] $label hook failed (exit $rc)${tail:+; $tail}" >&2
        exit "$rc"
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
# The agent home is bind-mounted from jackin's per-instance data dir so
# conversation history and runtime-local plugins survive Docker container
# loss. The derived image stores its baked defaults under
# /jackin/default-home; seed_home_dir copies only missing files so the first
# launch gets the image defaults without clobbering state from prior runs.
# Auth handoff files still arrive under /jackin/<agent>/... and are copied
# into the durable home on every launch so the current auth mode wins.
case "${JACKIN_AGENT:?JACKIN_AGENT must be set}" in
  claude)
    seed_home_dir /jackin/default-home/.claude /home/agent/.claude
    if [ -f /jackin/claude/account.json ]; then
        cp /jackin/claude/account.json /home/agent/.claude.json
        chmod 600 /home/agent/.claude.json
    fi
    if [ -f /jackin/claude/credentials.json ]; then
        cp /jackin/claude/credentials.json /home/agent/.claude/.credentials.json
        chmod 600 /home/agent/.claude/.credentials.json
    else
        rm -f /home/agent/.claude/.credentials.json
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
    if [ "$#" -gt 0 ]; then
        LAUNCH+=("$@")
    fi
    ;;
  codex)
    seed_home_dir /jackin/default-home/.codex /home/agent/.codex
    if [ -f /jackin/codex/auth.json ]; then
        cp /jackin/codex/auth.json /home/agent/.codex/auth.json
        chmod 600 /home/agent/.codex/auth.json
    else
        rm -f /home/agent/.codex/auth.json
    fi
    LAUNCH=(codex --enable goals --dangerously-bypass-approvals-and-sandbox)
    if [ "$#" -gt 0 ]; then
        LAUNCH+=("$@")
    fi
    ;;
  amp)
    seed_home_dir /jackin/default-home/.local/share/amp /home/agent/.local/share/amp
    if [ -f /jackin/amp/secrets.json ]; then
        echo "[entrypoint] amp: forwarding host secrets.json into ~/.local/share/amp/" >&2
        cp /jackin/amp/secrets.json /home/agent/.local/share/amp/secrets.json
        chmod 600 /home/agent/.local/share/amp/secrets.json
    elif [ -n "${AMP_API_KEY:-}" ]; then
        echo "[entrypoint] amp: AMP_API_KEY present in env; agent will use api-key auth" >&2
    else
        rm -f /home/agent/.local/share/amp/secrets.json
        echo "[entrypoint] amp: no secrets.json mounted and AMP_API_KEY unset — agent will require interactive login" >&2
    fi
    # CLI flag chosen over `amp.dangerouslyAllowAll: true` so jackin
    # doesn't write to the operator's XDG_CONFIG.
    LAUNCH=(amp --dangerously-allow-all)
    ;;
  kimi)
    seed_home_dir /jackin/default-home/.kimi /home/agent/.kimi
    if [ -d /jackin/kimi ] && [ "$(ls -A /jackin/kimi 2>/dev/null)" ]; then
        echo "[entrypoint] kimi: copying provisioned credentials into ~/.kimi/" >&2
        cp -a /jackin/kimi/. /home/agent/.kimi/
    elif [ -d /jackin/kimi ]; then
        echo "[entrypoint] kimi: sync mode active but host ~/.kimi was absent at provision time — Kimi will start without forwarded auth" >&2
    elif [ -n "${KIMI_API_KEY:-}" ]; then
        echo "[entrypoint] kimi: KIMI_API_KEY present in env; agent will use api-key auth" >&2
    else
        echo "[entrypoint] kimi: KIMI_API_KEY unset — agent will require interactive login or config" >&2
    fi
    LAUNCH=(kimi --yolo)
    ;;
  opencode)
    seed_home_dir /jackin/default-home/.local/share/opencode /home/agent/.local/share/opencode
    if [ -f /jackin/opencode/auth.json ]; then
        echo "[entrypoint] opencode: forwarding host auth.json into ~/.local/share/opencode/" >&2
        cp /jackin/opencode/auth.json /home/agent/.local/share/opencode/auth.json
        chmod 600 /home/agent/.local/share/opencode/auth.json
    elif [ -n "${OPENCODE_API_KEY:-}" ]; then
        echo "[entrypoint] opencode: OPENCODE_API_KEY present in env; agent will use api-key auth" >&2
    else
        rm -f /home/agent/.local/share/opencode/auth.json
        echo "[entrypoint] opencode: no auth.json mounted and OPENCODE_API_KEY unset — agent will require interactive login" >&2
    fi
    mkdir -p /home/agent/.config/opencode
    if [ ! -f /home/agent/.config/opencode/opencode.json ]; then
        printf '%s\n' '{"permission":"allow"}' > /home/agent/.config/opencode/opencode.json
    fi
    LAUNCH=(opencode)
    if [ $# -gt 0 ]; then
        LAUNCH+=("$@")
    fi
    ;;
  *)
    echo "[entrypoint] unknown JACKIN_AGENT: $JACKIN_AGENT" >&2
    exit 2
    ;;
esac

# ── role runtime hooks ─────────────────────────────────────────────
if [ -x /jackin/runtime/hooks/setup-once.sh ]; then
    setup_once_marker="/jackin/state/hooks/setup-once.done"
    if [ ! -e "$setup_once_marker" ]; then
        if ! mkdir -p "$(dirname "$setup_once_marker")"; then
            echo "[entrypoint] failed to create marker directory $(dirname "$setup_once_marker")" >&2
            exit 1
        fi
        run_hook setup-once /jackin/runtime/hooks/setup-once.sh \
            "marker not written, will retry next launch"
        # Wrap touch so the post-success failure surfaces as an attributed
        # log; otherwise `set -e` aborts silently and the next launch
        # silently re-runs setup-once with no operator-visible reason.
        if ! touch "$setup_once_marker"; then
            echo "[entrypoint] setup-once succeeded but marker write failed; will retry next launch" >&2
            exit 1
        fi
    fi
fi

if [ -x /jackin/runtime/hooks/source.sh ]; then
    echo "[entrypoint] sourcing runtime hook..."
    # Hooks must use `return`, not `exit` (sourced `exit` kills the
    # entrypoint). Save PWD and clear any ERR trap the hook installs
    # so neither leaks into the exec'd agent.
    source_pwd="$PWD"
    # JACKIN_DEBUG xtrace would dump expanded `export SECRET=...` lines
    # from the sourced shell into operator logs; suspend around source.
    case $- in *x*) source_xtrace=1; set +x ;; esac
    # Capture rc before the test — `$?` after `if ! .` is 0.
    rc=0
    # shellcheck source=/dev/null
    . /jackin/runtime/hooks/source.sh || rc=$?
    if [ "$rc" -ne 0 ]; then
        echo "[entrypoint] source hook returned non-zero (exit $rc); aborting before agent launch" >&2
        exit "$rc"
    fi
    [ "${source_xtrace:-0}" = "1" ] && set -x
    trap - ERR
    if ! cd "$source_pwd"; then
        echo "[entrypoint] saved PWD ($source_pwd) vanished after source hook; falling back to /" >&2
        cd /
    fi
fi

if [ -x /jackin/runtime/hooks/preflight.sh ]; then
    run_hook preflight /jackin/runtime/hooks/preflight.sh
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
