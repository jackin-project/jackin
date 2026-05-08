#!/bin/bash
# agent-brown pre-launch hook. jackin's runtime entrypoint runs this
# script (via `[hooks].pre_launch` in jackin.role.toml) after the agent
# CLI is installed and plugins are bootstrapped, but before the agent
# itself launches.
#
# Why a runtime hook and not an image-build step: the Claude MCP
# registration needs the claude CLI, and the claude CLI is injected per
# container by jackin's derived-image build (Agent::install_block in
# jackin's src/agent/mod.rs), not baked into the published agent image.
# Trying to run `claude mcp add …` at image build time silently no-ops
# because the binary is not on PATH yet.
set -euo pipefail

# Line-numbered ERR trap so a future maintainer who wraps a function
# call in `if foo; then …` (which suspends `errexit`) still gets a
# diagnostic if the function later fails.
trap 'printf "[brown-pre-launch] ERROR: failed at line %s (exit %s)\n" "$LINENO" "$?" >&2; exit 1' ERR

log()  { printf '[brown-pre-launch] %s\n' "$*"; }
warn() { printf '[brown-pre-launch] WARNING: %s\n' "$*" >&2; }
err()  { printf '[brown-pre-launch] ERROR: %s\n'   "$*" >&2; }

# ── PostgreSQL host wiring ────────────────────────────────────────────
#
# Derive POSTGRESQL_DB_HOST from the DinD sidecar hostname so Gradle build
# files can connect to PostgreSQL running inside docker-compose.
# TODO: drop the DOCKER_HOST fallback below once jackin-project/jackin#32
# ships and JACKIN_DIND_HOSTNAME is always set.
if [ -n "${JACKIN_DIND_HOSTNAME:-}" ]; then
    export POSTGRESQL_DB_HOST="$JACKIN_DIND_HOSTNAME"
elif [ -n "${DOCKER_HOST:-}" ]; then
    export POSTGRESQL_DB_HOST
    POSTGRESQL_DB_HOST="$(echo "$DOCKER_HOST" | sed 's|tcp://||;s|:.*||')"
    if [ -z "${POSTGRESQL_DB_HOST}" ]; then
        err "failed to derive POSTGRESQL_DB_HOST from DOCKER_HOST=${DOCKER_HOST}"
        exit 1
    fi
fi

# ── Context7 MCP setup ────────────────────────────────────────────────
if [ -n "${CONTEXT7_API_KEY:-}" ]; then
    log "configuring Context7 MCP server"
    # API key passed via env to keep it out of /proc/<pid>/cmdline.
    if ! CONTEXT7_API_KEY="$CONTEXT7_API_KEY" mise exec -- ctx7 setup --claude --mcp -y; then
        err "ctx7 setup failed (exit $?). Check CONTEXT7_API_KEY and reachability of context7.com"
        exit 1
    fi
fi

# ── caveman-shrink MCP middleware proxy ───────────────────────────────
#
# caveman-shrink wraps another MCP server's stdio and caveman-compresses
# the responses before they reach the agent — used to cut the token
# cost of verbose MCP servers (filesystem, github, search, etc.). By
# itself the registration is a placeholder that spawns the upstream
# package; you wire it against a specific server later by editing the
# `mcpServers` entry to put the real upstream after the caveman-shrink
# arg list, e.g.:
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
    # The script header documents the runtime contract: when JACKIN_AGENT=claude,
    # jackin's derived-image build guarantees the claude CLI is on PATH before
    # this hook runs. A missing CLI here means that contract was violated.
    if ! command -v claude >/dev/null 2>&1; then
        err "JACKIN_AGENT=claude but claude CLI not on PATH — jackin install_block failed"
        exit 1
    fi

    # Idempotent: skip on subsequent container starts. `claude mcp get`
    # exits non-zero if the entry doesn't exist; capture stderr so an
    # auth/parse/crash error is distinguished from the benign "not found"
    # case. The benign message is "No MCP server with name …".
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
    if [ -f "${HOME}/.agents/skills/caveman/SKILL.md" ]; then
        log "codex caveman skills present in ${HOME}/.agents/skills"
        return 0
    fi

    err "codex caveman skill missing from ${HOME}/.agents/skills"
    err "this image is broken — rebuild docker.io/chainargos/jackin-agent-brown:latest or run: npx -y skills add JuliusBrussee/caveman -a codex --yes --global"
    exit 1
}

# Per-agent dispatch. An empty or unknown JACKIN_AGENT surfaces a
# warning instead of silently no-op-ing, so a misspelling or a
# regression in jackin's env-injection is visible at boot.
case "${JACKIN_AGENT:-}" in
    claude) register_caveman_shrink ;;
    codex)  verify_codex_caveman_skills ;;
    "")     warn "JACKIN_AGENT unset — skipping per-agent setup" ;;
    *)      warn "unknown JACKIN_AGENT=${JACKIN_AGENT} — skipping per-agent setup" ;;
esac
