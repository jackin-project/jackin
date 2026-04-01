#!/bin/bash
set -euo pipefail

plugins_file="/home/claude/.jackin/plugins.json"

run_maybe_quiet() {
    if [ "${CLAUDE_DEBUG:-0}" = "1" ]; then
        "$@"
    else
        "$@" > /dev/null 2>&1
    fi
}

if [ ! -f "$plugins_file" ]; then
    exit 0
fi

run_maybe_quiet claude plugin marketplace add anthropics/claude-plugins-official || true

jq -r '.plugins[]?' "$plugins_file" | while IFS= read -r plugin; do
    [ -n "$plugin" ] || continue
    run_maybe_quiet claude plugin install "$plugin"
done
