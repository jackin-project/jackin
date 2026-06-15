#!/bin/bash
# Runs after jackin installs the agent CLI and bootstraps plugins.
# `claude mcp add …` belongs here, not in the Dockerfile, because
# jackin injects the claude CLI per container at launch time.
set -euo pipefail

trap 'printf "[brown-preflight] ERROR: failed at line %s (exit %s)\n" "$LINENO" "$?" >&2; exit 1' ERR

log()  { printf '[brown-preflight] %s\n' "$*"; }
warn() { printf '[brown-preflight] WARNING: %s\n' "$*" >&2; }
err()  { printf '[brown-preflight] ERROR: %s\n'   "$*" >&2; }

# Configure Context7 for the active agent when CONTEXT7_API_KEY is set.
# Without an API key the launch is treated as Context7-disabled.
#
# Args: label, then ctx7 CLI flags selecting the agent and mode.
setup_context7() {
    local agent_label="$1"
    shift

    if [[ -z "${CONTEXT7_API_KEY:-}" ]]; then
        log "CONTEXT7_API_KEY unset — skipping Context7 setup for ${agent_label}"
        return 0
    fi

    log "configuring Context7 for ${agent_label}"
    if ! CONTEXT7_API_KEY="$CONTEXT7_API_KEY" mise exec -- ctx7 setup "$@" --yes; then
        err "ctx7 setup failed (exit $?). Check CONTEXT7_API_KEY and reachability of context7.com"
        exit 1
    fi
}

# caveman-shrink wraps another MCP server's stdio and compresses the
# responses to cut token cost. The registration here is a placeholder
# — wire it to a real upstream by editing the `mcpServers` entry to
# append the upstream after the caveman-shrink arg list, e.g.:
#
#   {
#     "mcpServers": {
#       "fs-shrunk": {
#         "command": "npx",
#         "args": ["caveman-shrink", "npx",
#                  "@modelcontextprotocol/server-filesystem", "/path"]
#       }
#     }
#   }
#
# Reference: https://github.com/JuliusBrussee/caveman/tree/main/mcp-servers/caveman-shrink
register_caveman_shrink() {
    # JACKIN_AGENT=claude is contracted to mean the claude CLI is on
    # PATH; a missing binary here is a contract violation, not a skip.
    if ! command -v claude >/dev/null 2>&1; then
        err "JACKIN_AGENT=claude but claude CLI not on PATH — jackin runtime contract violated"
        exit 1
    fi

    # Capture stderr so the benign "No MCP server with name …" case
    # is distinguished from a real auth/parse/crash error.
    local mcp_get_err
    if mcp_get_err="$(claude mcp get caveman-shrink 2>&1 >/dev/null)"; then
        return 0
    fi
    if [[ "$mcp_get_err" != *"No MCP server"* ]]; then
        err "claude mcp get failed unexpectedly: $mcp_get_err"
        exit 1
    fi

    log "registering caveman-shrink MCP server"
    claude mcp add caveman-shrink -- npx -y caveman-shrink
}

verify_codex_caveman_skills() {
    if [[ -f "${HOME}/.agents/skills/caveman/SKILL.md" ]]; then
        log "codex caveman skills present in ${HOME}/.agents/skills"
        return 0
    fi

    err "codex caveman skill missing from ${HOME}/.agents/skills"
    err "this image is broken — rebuild docker.io/chainargos/jackin-agent-brown:latest or run: npx -y skills add JuliusBrussee/caveman -a codex --yes --global"
    exit 1
}

case "${JACKIN_AGENT:-}" in
    claude)
        register_caveman_shrink
        setup_context7 claude --claude --mcp
        ;;
    codex)
        verify_codex_caveman_skills
        setup_context7 codex --codex --mcp
        ;;
    opencode)
        setup_context7 opencode --opencode --mcp
        ;;
    amp|kimi)
        setup_context7 "$JACKIN_AGENT" --cli --universal
        ;;
    "")     warn "JACKIN_AGENT unset — skipping per-agent setup" ;;
    *)      warn "unknown JACKIN_AGENT=${JACKIN_AGENT} — skipping per-agent setup" ;;
esac
