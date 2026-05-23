#!/bin/bash
set -euo pipefail
# Skip amend (-c/-C/--amend all pass $2=commit) and squash:
# the original or consolidated message already has the trailers.
case "${2:-}" in
  commit|squash) exit 0 ;;
esac

# Ensure $2 is in the final Git trailer block. GitHub only renders
# co-authors from that parseable block, so exact existing copies are
# removed before re-adding through Git's trailer parser.
_append_trailer() {
    _tmp="$(mktemp)" || {
        echo "[jackin prepare-commit-msg] ERROR: failed to create tempfile while appending $3" >&2
        exit 1
    }
    if ! awk -v trailer="$2" '$0 != trailer { print }' "$1" > "$_tmp"; then
        rm -f "$_tmp"
        echo "[jackin prepare-commit-msg] ERROR: failed to normalize existing $3 trailer in $1" >&2
        exit 1
    fi
    if ! cat "$_tmp" > "$1"; then
        rm -f "$_tmp"
        echo "[jackin prepare-commit-msg] ERROR: failed to rewrite $1 while appending $3" >&2
        exit 1
    fi
    rm -f "$_tmp"
    if ! git interpret-trailers --in-place --if-exists=addIfDifferent --trailer "$2" "$1"; then
        echo "[jackin prepare-commit-msg] ERROR: failed to append $3 to $1" >&2
        exit 1
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
