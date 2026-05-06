#!/bin/bash
set -euo pipefail

# The plugins manifest path is required as the first argument. Production
# callers pass `/jackin/claude/plugins.json` (the location jackin
# bind-mounts the per-role manifest into); tests pass a temp-dir path.
# Keeping it explicit avoids env-default-override complexity and makes
# the data dependency obvious at the call site.
if [ "$#" -ne 1 ]; then
    echo "usage: install-claude-plugins.sh <plugins_file>" >&2
    exit 2
fi
plugins_file="$1"

run_maybe_quiet() {
    if [ "${JACKIN_DEBUG:-0}" = "1" ]; then
        "$@"
    else
        "$@" > /dev/null
    fi
}

if [ ! -f "$plugins_file" ]; then
    exit 0
fi

# Build a set of already-installed plugin IDs so we can skip them.
installed_ids=""
if claude plugin list --json > /dev/null 2>&1; then
    installed_ids=$(claude plugin list --json 2>/dev/null | jq -r '.[].id' 2>/dev/null || true)
fi

is_installed() {
    local plugin_id="$1"
    echo "$installed_ids" | grep -qxF "$plugin_id"
}

claude plugin marketplace add anthropics/claude-plugins-official > /dev/null 2>&1 || true

jq -c '.marketplaces[]?' "$plugins_file" | while IFS= read -r marketplace; do
    [ -n "$marketplace" ] || continue
    source=$(printf '%s' "$marketplace" | jq -r '.source')
    args=(claude plugin marketplace add "$source")
    sparse_paths=()
    while IFS= read -r sparse; do
        [ -n "$sparse" ] || continue
        sparse_paths+=("$sparse")
    done < <(printf '%s' "$marketplace" | jq -r '.sparse[]?')
    if [ "${#sparse_paths[@]}" -gt 0 ]; then
        args+=(--sparse "${sparse_paths[@]}")
    fi
    run_maybe_quiet "${args[@]}"
done

jq -r '.plugins[]?' "$plugins_file" | while IFS= read -r plugin; do
    [ -n "$plugin" ] || continue
    if is_installed "$plugin"; then
        if [ "${JACKIN_DEBUG:-0}" = "1" ]; then
            echo "Plugin already installed: $plugin"
        fi
        continue
    fi
    run_maybe_quiet claude plugin install "$plugin"
done
