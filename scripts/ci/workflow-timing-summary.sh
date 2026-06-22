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
metrics_file="${RUNNER_TEMP:-/tmp}/workflow-target-metrics-${run_id}.tsv"

: > "$jobs_file"
: > "$durations_file"
: > "$steps_file"
: > "$cache_file"
: > "$metrics_file"

epoch() {
  local value="$1"
  if [ -z "$value" ] || [ "$value" = "null" ] || [[ "$value" == 0001-* ]]; then
    echo 0
    return
  fi
  date -u -d "$value" +%s
}

duration() {
  local seconds="$1"
  printf '%dm %02ds' "$((seconds / 60))" "$((seconds % 60))"
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
  if [ "$end_s" -gt 0 ]; then
    printf '%s\t%s\t%s\t%s\t%s\n' \
      "$end_s" "$name" "$conclusion" "$status" "$url" >> "$metrics_file"
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

first_red_metric() {
  awk -F '\t' '
    $3 != "" && $3 != "success" && $3 != "skipped" {
      if (best == "" || $1 < best) {
        best = $1
        name = $2
        result = $3
        url = $5
      }
    }
    END {
      if (best != "") printf "%s\t%s\t%s\t%s\n", best, name, result, url
    }
  ' "$metrics_file"
}

job_completion_metric() {
  local pattern="$1"
  awk -F '\t' -v pattern="$pattern" '
    $2 ~ pattern && $3 != "" && $3 != "skipped" {
      if (best == "" || $1 > best) {
        best = $1
        name = $2
        result = $3
        url = $5
      }
    }
    END {
      if (best != "") printf "%s\t%s\t%s\t%s\n", best, name, result, url
    }
  ' "$metrics_file"
}

print_elapsed_metric() {
  local label="$1"
  local metric="$2"

  if [ -z "$metric" ]; then
    printf -- '- %s: unavailable\n' "$label"
    return
  fi

  IFS=$'\t' read -r completed_s name result url <<< "$metric"
  if [ "$run_start" -gt 0 ] && [ "$completed_s" -ge "$run_start" ]; then
    printf -- '- %s: %s via [%s](%s) (%s)\n' \
      "$label" "$(duration "$((completed_s - run_start))")" "$name" "$url" "$result"
  else
    printf -- '- %s: unavailable via [%s](%s) (%s)\n' "$label" "$name" "$url" "$result"
  fi
}

print_target_metrics() {
  printf '\n### Target metrics\n\n'

  local first_red
  first_red=$(first_red_metric)
  if [ -n "$first_red" ]; then
    print_elapsed_metric "Time to first red signal" "$first_red"
  else
    printf -- '- Time to first red signal: none; no completed job failed\n'
  fi

  case "$workflow_label" in
    CI)
      print_elapsed_metric "Time to CI required gate" "$(job_completion_metric '^ci-required$')"
      ;;
    Docs)
      print_elapsed_metric "Time to docs required gate" "$(job_completion_metric '^docs-required$')"
      ;;
    "Construct Image")
      print_elapsed_metric "Time to construct required gate" "$(job_completion_metric '^construct-required$')"
      ;;
    "Publish Homebrew Preview")
      print_elapsed_metric "Time to preview published" "$(job_completion_metric '^publish-preview$')"
      ;;
    Release)
      print_elapsed_metric "Time to GitHub release published" "$(job_completion_metric '^release$')"
      print_elapsed_metric "Time to release pipeline complete" "$(job_completion_metric '^homebrew$|^release$')"
      ;;
  esac
}

print_step_category_totals() {
  printf '\n### Step category totals\n\n'
  printf '| Category | Total | Steps |\n| --- | --- | --- |\n'

  awk -F '\t' '
    function category(step) {
      if (step ~ /Cache|cache|rust-cache|Restore|Save/) return "cache"
      if (step ~ /jdx\/mise-action|mise|rustup|Set up job/) return "tool setup"
      if (step ~ /upload-artifact|download-artifact|Upload|Download/) return "artifacts"
      if (step ~ /Docker|Buildx|construct image|docker /) return "docker"
      if (step ~ /cargo |Cargo |nextest|clippy|fmt|audit|deny|shear|fuzz|bench|schema/) return "cargo"
      return "other"
    }
    {
      cat = category($3)
      seconds[cat] += $1
      count[cat] += 1
    }
    END {
      for (cat in seconds) {
        printf "%d\t%s\t%d\n", seconds[cat], cat, count[cat]
      }
    }
  ' "$steps_file" | sort -rn | while IFS=$'\t' read -r seconds category count; do
    printf '| %s | %s | %s |\n' "$category" "$(duration "$seconds")" "$count"
  done
}

{
  printf '## %s timing\n\n' "$workflow_label"
  if [ "$run_start" -gt 0 ] && [ "$run_end" -gt "$run_start" ]; then
    printf -- '- Workflow wall clock: %s\n' "$(duration "$((run_end - run_start))")"
  else
    printf -- '- Workflow wall clock: unavailable while jobs are still running\n'
  fi
  printf -- '- Cache events in job logs: %s hit/restore markers, %s miss markers, %s restore attempts, %s failure markers\n\n' \
    "$cache_hit_count" "$cache_miss_count" "$cache_restore_count" "$cache_failure_count"

  print_target_metrics

  printf '### Longest jobs\n\n'
  printf '| Duration | Job | Result |\n| --- | --- | --- |\n'
  sort -rn "$durations_file" | awk 'NR <= 10' | while IFS=$'\t' read -r seconds name conclusion status url; do
    printf '| %s | [%s](%s) | %s |\n' \
      "$(duration "$seconds")" "$name" "$url" "${conclusion:-$status}"
  done

  printf '\n### Longest steps\n\n'
  printf '| Duration | Job | Step | Result |\n| --- | --- | --- | --- |\n'
  sort -rn "$steps_file" | awk 'NR <= 10' | while IFS=$'\t' read -r seconds job step conclusion; do
    printf '| %s | %s | %s | %s |\n' \
      "$(duration "$seconds")" "$job" "$step" "$conclusion"
  done

  print_step_category_totals

  if [ -s "$cache_file" ]; then
    printf '\n### Notes\n\n'
    sed 's/^/- /' "$cache_file"
  fi
} >> "$summary"
