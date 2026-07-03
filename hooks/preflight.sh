#!/bin/bash
# Per-agent setup after jackin bootstrap (claude mcp etc. here).
set -euo pipefail

trap 'printf "[brown-preflight] ERROR: failed at line %s (exit %s)\n" "$LINENO" "$?" >&2; exit 1' ERR

log()  { printf '[brown-preflight] %s\n' "$*"; }
warn() { printf '[brown-preflight] WARNING: %s\n' "$*" >&2; }
err()  { printf '[brown-preflight] ERROR: %s\n'   "$*" >&2; }

json_set() {
    local file="$1"; shift
    local dir; dir="$(dirname "${file}")"
    mkdir -p "${dir}"
    [[ -f "${file}" ]] || printf '{}\n' > "${file}"

    local tmp; tmp="$(mktemp "${dir}/.$(basename "${file}").XXXXXX")"
    if jq "$@" "${file}" > "${tmp}"; then
        mv "${tmp}" "${file}"
        return 0
    fi
    rm -f "${tmp}"
    return 1
}

setup_context7() {
    local agent_label="$1"
    shift
    if [[ -z "${CONTEXT7_API_KEY:-}" ]]; then return 0; fi
    if ! mise exec -- ctx7 setup "$@" --api-key "$CONTEXT7_API_KEY" -y; then
        err "ctx7 setup failed"
        exit 1
    fi
}

# Context7 MCP for agents ctx7 setup does not target (amp/kimi/grok).
# ctx7 setup (v0.5.3) has no --amp/--kimi/--grok flag, so --cli writes no MCP
# config — the agent gets only the ctx7 binary with no integration. Wire the
# server directly per agent, mirroring the headroom wiring pattern below.
setup_context7_native_mcp() {
    local agent="$1"

    if [[ -z "${CONTEXT7_API_KEY:-}" ]]; then
        log "CONTEXT7_API_KEY unset — skipping Context7 MCP for ${agent}"
        return 0
    fi

    log "configuring Context7 MCP (native) for ${agent}"
    case "${agent}" in
        amp)
            # amp settings.json uses a flat "amp.mcpServers" key; HTTP is auto-detected.
            local f="${HOME}/.config/amp/settings.json"
            json_set "${f}" --arg key "${CONTEXT7_API_KEY}" \
                '.["amp.mcpServers"].context7 = {"url":"https://mcp.context7.com/mcp","headers":{"CONTEXT7_API_KEY":$key}}' \
                || { err "amp Context7 MCP wire failed"; exit 1; }
            ;;
        kimi)
            # kimi global MCP lives in ~/.kimi-code/mcp.json (not config.toml).
            local f="${HOME}/.kimi-code/mcp.json"
            json_set "${f}" --arg key "${CONTEXT7_API_KEY}" \
                '.mcpServers.context7 = {"url":"https://mcp.context7.com/mcp","headers":{"CONTEXT7_API_KEY":$key}}' \
                || { err "kimi Context7 MCP wire failed"; exit 1; }
            ;;
        grok)
            # grok's streamable-HTTP client wraps every remote in an OAuth AuthClient;
            # only Authorization is treated as static-bearer. context7 accepts
            # Authorization: Bearer (verified), so use it to avoid the OAuth collision.
            grok mcp remove context7 >/dev/null 2>&1 || true
            grok mcp add --transport http context7 https://mcp.context7.com/mcp \
                --header "Authorization: Bearer ${CONTEXT7_API_KEY}" \
                || { err "grok Context7 MCP wire failed"; exit 1; }
            ;;
    esac
}

seed_caveman_flag() {
    local claude_dir="${CLAUDE_CONFIG_DIR:-${HOME}/.claude}"
    local flag="${claude_dir}/.caveman-active"
    mkdir -p "${claude_dir}"
    printf '%s' "${CAVEMAN_DEFAULT_MODE:-ultra}" > "${flag}"
}

wire_caveman_statusline() {
    local claude_dir="${CLAUDE_CONFIG_DIR:-${HOME}/.claude}"
    local settings="${claude_dir}/settings.json"
    if ! command -v jq >/dev/null 2>&1; then return 0; fi
    local current=""
    [[ -f "${settings}" ]] && current="$(jq -r '.statusLine.command // ""' "${settings}" 2>/dev/null || echo "")"
    if [[ "${current}" == *caveman-statusline.sh* ]]; then return 0; fi
    if [[ -n "${current}" ]]; then return 0; fi
    local script
    script="$(find "${claude_dir}/plugins/marketplaces/caveman" "${claude_dir}/plugins/cache/caveman" -name caveman-statusline.sh -type f 2>/dev/null | head -n1)"
    if [[ -z "${script}" ]]; then return 0; fi
    if json_set "${settings}" --arg cmd "bash ${script}" '.statusLine = {"type": "command", "command": $cmd}'; then :; fi
}

register_rtk_hook() {
    if ! command -v rtk >/dev/null 2>&1; then return 0; fi
    rtk init -g --hook-only --auto-patch || warn "rtk hook failed"
}

register_headroom_mcp() {
    if ! command -v headroom >/dev/null 2>&1; then return 0; fi
    case "${JACKIN_AGENT:-}" in
        claude)
            if mcp_err="$(claude mcp get headroom 2>&1 >/dev/null)"; then return 0; fi
            if [[ "$mcp_err" != *"No MCP server"* ]]; then err "headroom failed"; exit 1; fi
            headroom mcp install --quiet 2>/dev/null || claude mcp add headroom -- headroom mcp serve
            ;;
        codex|amp|grok)
            "${JACKIN_AGENT}" mcp add headroom -- headroom mcp serve 2>/dev/null || true
            ;;
        opencode|kimi)
            patch_headroom_json "${JACKIN_AGENT}"
            ;;
    esac
}

patch_headroom_json() {
    local target="$1" file filter
    if ! command -v jq >/dev/null 2>&1; then return 0; fi
    case "${target}" in
        opencode) file="${HOME}/.config/opencode/opencode.json"; filter='.mcp.headroom = {"type":"local","command":["headroom","mcp","serve"],"enabled":true}' ;;
        kimi) file="${HOME}/.kimi-code/mcp.json"; filter='.mcpServers.headroom = {"command":"headroom","args":["mcp","serve"]}' ;;
        *) return 0 ;;
    esac
    json_set "${file}" "${filter}" || true
}

verify_codex_caveman_skills() {
    if [[ -f "${HOME}/.agents/skills/caveman/SKILL.md" ]]; then return 0; fi
    err "codex caveman skill missing"
    err "run: npx -y skills add JuliusBrussee/caveman -a codex --yes --global"
    exit 1
}

case "${JACKIN_AGENT:-}" in
    claude)
        seed_caveman_flag
        wire_caveman_statusline
        register_rtk_hook
        setup_context7 claude --claude --mcp
        register_headroom_mcp
        ;;
    codex)
        verify_codex_caveman_skills
        setup_context7 codex --codex --mcp
        register_headroom_mcp
        ;;
    opencode)
        setup_context7 opencode --opencode --mcp
        register_headroom_mcp
        ;;
    amp|kimi|grok)
        setup_context7_native_mcp "$JACKIN_AGENT"
        register_headroom_mcp
        ;;
    "")     warn "JACKIN_AGENT unset — skipping per-agent setup" ;;
    *)      warn "unknown JACKIN_AGENT=${JACKIN_AGENT} — skipping per-agent setup" ;;
esac
