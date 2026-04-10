#!/bin/bash
set -euo pipefail

plugins_file="${JACKIN_PLUGINS_FILE:-/home/claude/.jackin/plugins.json}"

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
    run_maybe_quiet claude plugin install "$plugin"
done
