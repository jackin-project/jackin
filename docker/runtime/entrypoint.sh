#!/bin/bash
set -euo pipefail

# Trace all commands in debug mode
if [ "${JACKIN_DEBUG:-0}" = "1" ]; then
    set -x
fi

# Run a child-process hook with a `[entrypoint]` log prefix and an
# explicit failure-attributed exit. `$3` (optional) is appended after
# `; ` to the failure line — used by setup-once to surface its retry
# semantics.
run_hook() {
    local label="$1" path="$2" tail="${3:-}" hook_cwd="${4:-}"
    echo "[entrypoint] running $label hook..."
    # `$?` inside `if ! cmd; then ...` is the negated test's status (0),
    # not the hook's. Capture before the test so the failure log + exit
    # surface the real exit code.
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

# Deterministic runtime setup lives in Rust now: global git identity,
# SSH->HTTPS rewrite, gh credential helper setup, and the shared git
# trailer hook, plus per-agent home/auth preparation. This keeps
# repeated pane starts quiet and avoids shell xtrace leaking token
# values.
/jackin/runtime/jackin-capsule runtime-setup

# ── agent-specific setup ───────────────────────────────────────────
#
# Per-session file setup already ran in `jackin-capsule runtime-setup`.
# The remaining shell branch only builds the final argv because role
# `source.sh` can mutate this shell's environment before `exec`.
# ── jackin-exec system prompt injection ────────────────────────────
# When on-demand credential bindings are configured (JACKIN_EXEC_BINDINGS
# is non-empty), prepend a system prompt block telling agents to use
# jackin-exec for those commands.
JACKIN_EXEC_SYSTEM_PROMPT=""
if [ -n "${JACKIN_EXEC_BINDINGS:-}" ]; then
    JACKIN_EXEC_SYSTEM_PROMPT="$(cat <<'EXEC_PROMPT'
When you need to execute commands that require credentials — SSH connections,
registry logins, cloud CLI commands (aws, kubectl, gh), git push — call
jackin-exec <command> instead of running the command directly. jackin' will
securely inject the required credentials. Available secure bindings:
EXEC_PROMPT
)"
    # Append the binding names on a new line
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
    if [ -n "${JACKIN_CODEX_PROFILE:-}" ]; then
        LAUNCH+=(--profile "$JACKIN_CODEX_PROFILE")
        # Activate the provider's model catalog (real metadata + context window)
        # if runtime-setup wrote one. Passed as -c, not a profile-file key: a
        # profile-file model_catalog_json trips a Codex config-parse bug.
        catalog="$HOME/.codex/${JACKIN_CODEX_PROFILE}.models.json"
        if [ -f "$catalog" ]; then
            LAUNCH+=(-c "model_catalog_json=$catalog")
        fi
    fi
    if [ "$#" -gt 0 ]; then
        LAUNCH+=("$@")
    fi
    ;;
  amp)
    # CLI flag chosen over `amp.dangerouslyAllowAll: true` so jackin
    # doesn't write to the operator's XDG_CONFIG.
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
    # --always-approve auto-approves edits/tools (like --dangerously-*-*
    # for Claude/Amp/Kimi/etc.).
    # Role manifest model (if any) is passed via -m/--model in the
    # appended "$@" (from agent_model_args).
    # Other flags (plan mode etc.) can come via hooks or extra args.
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
        echo "[entrypoint] saved PWD ($source_pwd) vanished after source hook; cannot launch agent" >&2
        exit 1
    fi
fi

if [ -x /jackin/runtime/hooks/preflight.sh ]; then
    run_hook preflight /jackin/runtime/hooks/preflight.sh "" "$HOME"
fi

# In debug mode, pause so the operator can review logs before the agent clears the screen.
# Guard with `[ -t 0 ]` so a non-tty caller (test harness, CI smoke) does not
# trip `set -e` when `read` returns non-zero on a closed stdin.
if [ "${JACKIN_DEBUG:-0}" = "1" ] && [ -t 0 ]; then
    set +x
    echo ""
    echo "[entrypoint] Setup complete. Press Enter to launch ${JACKIN_AGENT}..."
    read -r
    set -x
fi

exec "${LAUNCH[@]}"
