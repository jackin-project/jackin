#!/bin/bash
set -euo pipefail
# Skip amend (-c/-C/--amend all pass $2=commit), squash, and merge:
# the original or consolidated message already has the trailers.
case "${2:-}" in
  commit|squash|merge) exit 0 ;;
esac

# Append $2 trailer to commit-msg file $1 unless it is already present.
# If the last non-empty line is already a trailer (Key: value), append
# directly so the block stays contiguous. Otherwise prepend a blank line
# to separate the new trailer from the body.
_append_trailer() {
    if ! grep -qF "$2" "$1"; then
        _last=$(grep -v '^[[:space:]]*$' "$1" | tail -1)
        if printf '%s' "$_last" | grep -qE '^[A-Za-z-]+: .+'; then
            printf '%s\n' "$2" >> "$1"
        else
            printf '\n%s\n' "$2" >> "$1"
        fi || {
            echo "[jackin prepare-commit-msg] ERROR: failed to append $3 to $1" >&2
            exit 1
        }
    fi
}

# Co-authored-by (agent-specific, only if JACKIN_GIT_COAUTHOR_TRAILER=1).
if [ "${JACKIN_GIT_COAUTHOR_TRAILER:-0}" = "1" ]; then
    _agent="${JACKIN_AGENT:-}"
    _coauthor_trailer=""
    if [ "$_agent" = "claude" ]; then
        _coauthor_trailer="Co-authored-by: Claude <noreply@anthropic.com>"
    elif [ "$_agent" = "codex" ]; then
        _coauthor_trailer="Co-authored-by: Codex <codex@openai.com>"
    elif [ "$_agent" = "amp" ]; then
        _coauthor_trailer="Co-authored-by: Amp <amp@ampcode.com>"
    elif [ "$_agent" = "opencode" ]; then
        _coauthor_trailer="Co-authored-by: opencode-agent[bot] <opencode-agent[bot]@users.noreply.github.com>"
    fi
    # kimi intentionally absent: no canonical GitHub App identity in AGENTS.md.
    if [ -n "$_coauthor_trailer" ]; then
        _append_trailer "$1" "$_coauthor_trailer" "Co-authored-by"
    else
        echo "[jackin prepare-commit-msg] WARNING: JACKIN_GIT_COAUTHOR_TRAILER=1 but JACKIN_AGENT='${_agent}' is not a recognized agent slug; no Co-authored-by trailer written" >&2
    fi
fi

# Signed-off-by / DCO (from git identity, only if JACKIN_GIT_DCO=1).
if [ "${JACKIN_GIT_DCO:-0}" = "1" ]; then
    _dco_name="$(git config user.name 2>/dev/null || true)"
    _dco_email="$(git config user.email 2>/dev/null || true)"
    if [ -n "$_dco_name" ] && [ -n "$_dco_email" ]; then
        _append_trailer "$1" "Signed-off-by: ${_dco_name} <${_dco_email}>" "Signed-off-by"
    else
        echo "[jackin prepare-commit-msg] WARNING: JACKIN_GIT_DCO=1 but git identity is not configured (user.name='${_dco_name}' user.email='${_dco_email}'); no Signed-off-by trailer written" >&2
    fi
fi
