#!/usr/bin/env bash
set -euo pipefail

workflow_label="${1:?workflow label required}"
repo="${GITHUB_REPOSITORY:?GITHUB_REPOSITORY is required}"
run_id="${GITHUB_RUN_ID:?GITHUB_RUN_ID is required}"
summary="${GITHUB_STEP_SUMMARY:?GITHUB_STEP_SUMMARY is required}"

jobs_file="${RUNNER_TEMP:-/tmp}/workflow-jobs-${run_id}.jsonl"
durations_file="${RUNNER_TEMP:-/tmp}/workflow-job-durations-${run_id}.tsv"
steps_file="${RUNNER_TEMP:-/tmp}/workflow-step-durations-${run_id}.tsv"
cache_file="${RUNNER_TEMP:-/tmp}/workflow-cache-events-${run_id}.txt"

: > "$jobs_file"
: > "$durations_file"
: > "$steps_file"
: > "$cache_file"

epoch() {
  local value="$1"
  if [ -z "$value" ] || [ "$value" = "null" ] || [[ "$value" == 0001-* ]]; then
    echo 0
    return
  fi
  date -u -d "$value" +%s
}

gh api "repos/${repo}/actions/runs/${run_id}/jobs?per_page=100" \
  --paginate \
  --jq '.jobs[] | @base64' > "$jobs_file"

run_start=0
run_end=0
cache_hit_count=0
cache_miss_count=0
cache_restore_count=0
cache_failure_count=0

while IFS= read -r job_b64; do
  [ -n "$job_b64" ] || continue
  job_json=$(printf '%s' "$job_b64" | base64 --decode)
  job_id=$(printf '%s' "$job_json" | jq -r '.databaseId')
  name=$(printf '%s' "$job_json" | jq -r '.name')
  status=$(printf '%s' "$job_json" | jq -r '.status')
  conclusion=$(printf '%s' "$job_json" | jq -r '.conclusion // ""')
  url=$(printf '%s' "$job_json" | jq -r '.url')
  started_at=$(printf '%s' "$job_json" | jq -r '.startedAt // ""')
  completed_at=$(printf '%s' "$job_json" | jq -r '.completedAt // ""')
  start_s=$(epoch "$started_at")
  end_s=$(epoch "$completed_at")

  if [ "$start_s" -gt 0 ] && { [ "$run_start" -eq 0 ] || [ "$start_s" -lt "$run_start" ]; }; then
    run_start="$start_s"
  fi
  if [ "$end_s" -gt "$run_end" ]; then
    run_end="$end_s"
  fi
  if [ "$start_s" -gt 0 ] && [ "$end_s" -gt "$start_s" ]; then
    printf '%s\t%s\t%s\t%s\t%s\n' \
      "$((end_s - start_s))" "$name" "$conclusion" "$status" "$url" >> "$durations_file"
  fi

  printf '%s' "$job_json" | jq -r --arg job "$name" '
    .steps[]
    | select((.startedAt // "") != "" and (.completedAt // "") != "" and (.completedAt | startswith("0001-") | not))
    | [.startedAt, .completedAt, $job, .name, (.conclusion // "")]
    | @tsv
  ' | while IFS=$'\t' read -r step_start step_end step_job step_name step_conclusion; do
    step_start_s=$(epoch "$step_start")
    step_end_s=$(epoch "$step_end")
    if [ "$step_start_s" -gt 0 ] && [ "$step_end_s" -gt "$step_start_s" ]; then
      printf '%s\t%s\t%s\t%s\n' \
        "$((step_end_s - step_start_s))" "$step_job" "$step_name" "$step_conclusion" >> "$steps_file"
    fi
  done

  if logs=$(gh api "repos/${repo}/actions/jobs/${job_id}/logs" 2>/dev/null); then
    hits=$(printf '%s\n' "$logs" | grep -Eci 'Cache hit for:|Restored from cache key|Cache restored successfully' || true)
    misses=$(printf '%s\n' "$logs" | grep -Eci 'Cache not found|not found for input keys|Cache miss' || true)
    restores=$(printf '%s\n' "$logs" | grep -Eci 'Restoring cache|Cache Size:' || true)
    failures=$(printf '%s\n' "$logs" | grep -Eci 'Failed to restore cache|Failed to save cache|Unable to reserve cache' || true)
    cache_hit_count=$((cache_hit_count + hits))
    cache_miss_count=$((cache_miss_count + misses))
    cache_restore_count=$((cache_restore_count + restores))
    cache_failure_count=$((cache_failure_count + failures))
  else
    printf 'log unavailable: %s (%s)\n' "$name" "$job_id" >> "$cache_file"
  fi
done < "$jobs_file"

{
  printf '## %s timing\n\n' "$workflow_label"
  if [ "$run_start" -gt 0 ] && [ "$run_end" -gt "$run_start" ]; then
    printf -- '- Workflow wall clock: %dm %02ds\n' "$(((run_end - run_start) / 60))" "$(((run_end - run_start) % 60))"
  else
    printf -- '- Workflow wall clock: unavailable while jobs are still running\n'
  fi
  printf -- '- Cache events in job logs: %s hit/restore markers, %s miss markers, %s restore attempts, %s failure markers\n\n' \
    "$cache_hit_count" "$cache_miss_count" "$cache_restore_count" "$cache_failure_count"

  printf '### Longest jobs\n\n'
  printf '| Duration | Job | Result |\n| --- | --- | --- |\n'
  sort -rn "$durations_file" | awk 'NR <= 10' | while IFS=$'\t' read -r seconds name conclusion status url; do
    printf '| %dm %02ds | [%s](%s) | %s |\n' \
      "$((seconds / 60))" "$((seconds % 60))" "$name" "$url" "${conclusion:-$status}"
  done

  printf '\n### Longest steps\n\n'
  printf '| Duration | Job | Step | Result |\n| --- | --- | --- | --- |\n'
  sort -rn "$steps_file" | awk 'NR <= 10' | while IFS=$'\t' read -r seconds job step conclusion; do
    printf '| %dm %02ds | %s | %s | %s |\n' \
      "$((seconds / 60))" "$((seconds % 60))" "$job" "$step" "$conclusion"
  done

  if [ -s "$cache_file" ]; then
    printf '\n### Notes\n\n'
    sed 's/^/- /' "$cache_file"
  fi
} >> "$summary"
