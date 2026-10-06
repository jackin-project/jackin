#!/bin/sh

set -u
verdict() { printf '%s\n' "TAILROCKS GOAL: $1"; case "$1" in PASS\ *) exit 0;; *) exit 1;; esac; }
script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" 2>/dev/null && pwd) || verdict "BLOCKED malformed=script-path"
slug=${1:-$(basename -- "$script_dir")}; package="plans/$slug"; hub="$package/README.md"; goal="$package/GOAL.md"
[ -f "$hub" ] || verdict "BLOCKED malformed=missing-readme"; [ -f "$goal" ] || verdict "BLOCKED malformed=missing-goal"
[ -z "$(git status --porcelain 2>/dev/null)" ] || verdict "BLOCKED dirty-tree"
expected_fingerprint=$(sed -n 's/^Frozen package fingerprint: `\([^`]*\)`.*/\1/p' "$hub" | sed -n '1p')
case "$expected_fingerprint" in ''|*[!0-9a-f]*) verdict "BLOCKED malformed=fingerprint";; esac
frozen_files=$(find "$package" -type f ! -path "$hub" -print 2>/dev/null | LC_ALL=C sort); [ -n "$frozen_files" ] || verdict "BLOCKED malformed=frozen-files"
actual_fingerprint=$(printf '%s\n' "$frozen_files" | while IFS= read -r file; do printf '%s %s\n' "$(git hash-object -- "$file")" "$file"; done | git hash-object --stdin) || verdict "BLOCKED malformed=fingerprint"
[ "$actual_fingerprint" = "$expected_fingerprint" ] || verdict "BLOCKED plan-drift"
status_counts=$(awk -F '|' '/^\|/ {s=$(NF-1); gsub(/^[[:space:]]+|[[:space:]]+$/, "", s); if(s=="DONE") d++; else if(s=="REJECTED"||s~/^REJECTED \(/) r++; else if(s=="TODO"||s=="STALE"||s=="IN PROGRESS"||s=="BLOCKED"||s~/^BLOCKED \(/) n++} END{print d+0,r+0,n+0}' "$hub")
set -- $status_counts; [ "$3" -eq 0 ] || verdict "BLOCKED nonterminal-rows=$3"; [ "$1" -gt 0 ] || verdict "BLOCKED malformed=status-table"
gates=$(awk '/^```sh gates[[:space:]]*$/{i=1;f=1;next} i&&/^```[[:space:]]*$/{i=0;c=1;exit} i{print} END{if(!f||!c)exit 1}' "$goal") || verdict "BLOCKED malformed=gates-block"
[ -n "$gates" ] || verdict "BLOCKED malformed=gates-block"
printf '%s\n' "$gates" | while IFS= read -r command; do [ -n "$command" ] || continue; sh -c "$command" || { printf '%s\n' "TAILROCKS GOAL: BLOCKED gate-failed=$command"; exit 1; }; done || exit $?
head_sha=$(git rev-parse --short HEAD 2>/dev/null) || verdict "BLOCKED malformed=head-sha"; verdict "PASS $head_sha"

