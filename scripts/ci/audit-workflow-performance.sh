#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Alexey Zhokhov
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

repo=${GITHUB_REPOSITORY:?GITHUB_REPOSITORY must be set}
run_id=${GITHUB_RUN_ID:?GITHUB_RUN_ID must be set}
summary=${GITHUB_STEP_SUMMARY:?GITHUB_STEP_SUMMARY must be set}
temp_root=${RUNNER_TEMP:?RUNNER_TEMP must be set}/workflow-performance-${run_id}
expect_clean=${EXPECT_CLEAN:-false}
workflow_label=${WORKFLOW_LABEL:-CI}
rm -rf "$temp_root"
mkdir -p "$temp_root"

jobs_file="$temp_root/jobs.jsonl"
rows_file="$temp_root/jobs.tsv"
steps_file="$temp_root/steps.tsv"
markers_file="$temp_root/markers.tsv"
: > "$rows_file"
: > "$steps_file"
: > "$markers_file"

gh api "repos/${repo}/actions/runs/${run_id}/jobs?per_page=100" \
  --paginate --jq '.jobs[] | @base64' > "$jobs_file"

epoch() {
  local timestamp=$1
  if [ -z "$timestamp" ] || [ "$timestamp" = null ] || [[ "$timestamp" == 0001-* ]]; then
    printf '0\n'
  else
    date -u -d "$timestamp" +%s
  fi
}

duration() {
  local seconds=$1
  printf '%dm %02ds' "$((seconds / 60))" "$((seconds % 60))"
}

download_pattern='Updating.*crates\.io index|Downloading crates|Downloaded.*[[:alnum:]_.+-]+ v[0-9]|info: downloading [0-9]+ components'
third_party_pattern='^[[:space:]]*(Compiling|Checking|Building) [[:alnum:]_.+-]+ v[0-9][^(/]*$'
source_tool_pattern='Installing [[:alnum:]_.+-]+ v[0-9].*from source'
cache_miss_pattern='Cache not found|No cache found|not found for input keys|cache miss'

run_created=$(gh api "repos/${repo}/actions/runs/${run_id}" --jq '.created_at')
run_created_s=$(epoch "$run_created")

log_pids=()
while IFS= read -r encoded; do
  [ -n "$encoded" ] || continue
  job=$(base64 --decode <<< "$encoded")
  id=$(jq -r '.id' <<< "$job")
  name=$(jq -r '.name' <<< "$job")
  status=$(jq -r '.status' <<< "$job")
  if [ "$status" = completed ]; then
    gh api "repos/${repo}/actions/jobs/${id}/logs" > "$temp_root/${id}.log" 2>/dev/null &
    log_pids+=("$!")
  fi
done < "$jobs_file"
for pid in "${log_pids[@]}"; do
  wait "$pid" || true
done

while IFS= read -r encoded; do
  [ -n "$encoded" ] || continue
  job=$(base64 --decode <<< "$encoded")
  id=$(jq -r '.id' <<< "$job")
  name=$(jq -r '.name' <<< "$job")
  status=$(jq -r '.status' <<< "$job")
  conclusion=$(jq -r '.conclusion // ""' <<< "$job")
  started=$(jq -r '.started_at // ""' <<< "$job")
  completed=$(jq -r '.completed_at // ""' <<< "$job")
  started_s=$(epoch "$started")
  completed_s=$(epoch "$completed")
  job_seconds=0
  queue_seconds=0
  if [ "$started_s" -gt 0 ] && [ "$completed_s" -gt "$started_s" ]; then
    job_seconds=$((completed_s - started_s))
  fi
  if [ "$started_s" -gt "$run_created_s" ]; then
    queue_seconds=$((started_s - run_created_s))
  fi

  longest_step='-'
  longest_step_seconds=0
  while IFS=$'\t' read -r step_name step_status step_started step_completed; do
    step_started_s=$(epoch "$step_started")
    step_completed_s=$(epoch "$step_completed")
    step_seconds=0
    if [ "$step_started_s" -gt 0 ] && [ "$step_completed_s" -gt "$step_started_s" ]; then
      step_seconds=$((step_completed_s - step_started_s))
    fi
    printf '%s\t%s\t%s\t%s\n' "$name" "$step_name" "$step_status" "$step_seconds" >> "$steps_file"
    if [ "$step_seconds" -gt "$longest_step_seconds" ]; then
      longest_step_seconds=$step_seconds
      longest_step=$step_name
    fi
  done < <(jq -r '.steps[] | [.name, (.conclusion // .status), .started_at, .completed_at] | @tsv' <<< "$job")

  downloads=0
  builds=0
  source_tools=0
  cache_misses=0
  if [ "$status" = completed ]; then
    log_file="$temp_root/${id}.log"
    normalized_file="$temp_root/${id}.normalized.log"
    if [ -s "$log_file" ]; then
      perl -pe 's/\e\[[0-9;]*[A-Za-z]//g; s/^[0-9]{4}-[0-9T:.-]+Z[[:space:]]+//; s/^[^\t]+\t[^\t]+\t[0-9]{4}-[0-9T:.-]+Z[[:space:]]+//' \
        "$log_file" > "$normalized_file"
      downloads=$(grep -Eci "$download_pattern" "$normalized_file" || true)
      builds=$(grep -Eci "$third_party_pattern" "$normalized_file" || true)
      source_tools=$(grep -Eci "$source_tool_pattern" "$normalized_file" || true)
      cache_misses=$(grep -Eci "$cache_miss_pattern" "$normalized_file" || true)
      grep -Ein "$download_pattern|$third_party_pattern|$source_tool_pattern" "$normalized_file" \
        | head -n 10 \
        | while IFS= read -r line; do
            printf '%s\t%s\n' "$name" "$line" >> "$markers_file"
          done || true
    fi
  fi

  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
    "$name" "${conclusion:-$status}" "$queue_seconds" "$job_seconds" \
    "$longest_step_seconds" "$longest_step" "$downloads" "$builds" \
    "$source_tools" "$cache_misses" >> "$rows_file"
done < "$jobs_file"

total_downloads=$(awk -F '\t' '{sum += $7} END {print sum + 0}' "$rows_file")
total_builds=$(awk -F '\t' '{sum += $8} END {print sum + 0}' "$rows_file")
total_source_tools=$(awk -F '\t' '{sum += $9} END {print sum + 0}' "$rows_file")
total_cache_misses=$(awk -F '\t' '{sum += $10} END {print sum + 0}' "$rows_file")

{
  printf '### %s performance audit\n' "$workflow_label"
  echo
  printf -- '- Dependency/toolchain download markers: %s\n' "$total_downloads"
  printf -- '- Third-party compile/check/build markers: %s\n' "$total_builds"
  printf -- '- Source-tool compile markers: %s\n' "$total_source_tools"
  printf -- '- Cache-miss markers: %s\n' "$total_cache_misses"
  echo
  echo '| Job | Result | Admission | Runtime | Longest step | Downloads | Third-party builds | Tool builds | Cache misses |'
  echo '| --- | --- | ---: | ---: | --- | ---: | ---: | ---: | ---: |'
  while IFS=$'\t' read -r name result queue_seconds job_seconds step_seconds step_name downloads builds source_tools cache_misses; do
    printf '| %s | %s | %s | %s | %s (%s) | %s | %s | %s | %s |\n' \
      "$name" "$result" "$(duration "$queue_seconds")" "$(duration "$job_seconds")" \
      "$step_name" "$(duration "$step_seconds")" "$downloads" "$builds" \
      "$source_tools" "$cache_misses"
  done < "$rows_file"
  echo
  echo '<details><summary>Every CI step duration</summary>'
  echo
  echo '| Job | Step | Result | Duration |'
  echo '| --- | --- | --- | ---: |'
  while IFS=$'\t' read -r job_name step_name step_status step_seconds; do
    printf '| %s | %s | %s | %s |\n' \
      "$job_name" "$step_name" "$step_status" "$(duration "$step_seconds")"
  done < "$steps_file"
  echo
  echo '</details>'
  if [ -s "$markers_file" ]; then
    echo
    echo '<details><summary>First forbidden download/build markers</summary>'
    echo
    echo '```text'
    head -n 30 "$markers_file"
    echo '```'
    echo '</details>'
  fi
} >> "$summary"

if [ "$expect_clean" = true ] &&
   [ "$((total_downloads + total_builds + total_source_tools + total_cache_misses))" -ne 0 ]; then
  echo "warm run emitted forbidden cache/dependency/tool markers" >&2
  exit 1
fi
