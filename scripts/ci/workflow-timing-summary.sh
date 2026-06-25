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
log_markers_file="${RUNNER_TEMP:-/tmp}/workflow-log-markers-${run_id}.tsv"
metrics_file="${RUNNER_TEMP:-/tmp}/workflow-target-metrics-${run_id}.tsv"

: > "$jobs_file"
: > "$durations_file"
: > "$steps_file"
: > "$cache_file"
: > "$log_markers_file"
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
dependency_download_count=0
third_party_build_count=0
source_tool_compile_count=0
sccache_issue_count=0
sccache_low_utility_count=0
prepared_workspace_count=0
velnor_job_log_artifact_count=0
github_cache_count=""
github_cache_bytes=""

dependency_download_pattern='Updating.*crates\.io index|Downloaded.*[0-9]+ crates?|Downloaded.*[[:alnum:]_.+-]+ v[0-9]|Downloading.*[[:alnum:]_.+-]+ v[0-9]'
third_party_build_pattern='^[[:space:]]*(Compiling|Checking|Building) [[:alnum:]_.+-]+ v[0-9][^(/]*$'
source_tool_compile_pattern='cargo install|Installing [[:alnum:]_.+-]+ v[0-9].*from source|Compiling [[:alnum:]_.+-]+ v[0-9].*\(.*cargo.*registry'
sccache_issue_pattern='sccache(:| )[[:alnum:] _-]*(error|failed)|Cache (read |write )?errors[[:space:]]+[1-9][0-9]*'
sccache_low_utility_pattern='Cache misses( \(Rust\))?[[:space:]]+[1-9][0-9]*|Cache hits rate( \(Rust\))?[[:space:]]+0\.00 %'
prepared_workspace_pattern='Download prepared nextest workspace|Restore prepared nextest workspace|prepared nextest workspace'
cache_miss_pattern='Cache not found|No cache found|not found for input keys'
cache_failure_pattern='Failed to restore cache|Failed to save cache|Unable to reserve cache'

if cache_usage_json=$(gh api "repos/${repo}/actions/cache/usage" 2>/dev/null); then
  github_cache_count=$(printf '%s' "$cache_usage_json" | jq -r '.active_caches_count // ""')
  github_cache_bytes=$(printf '%s' "$cache_usage_json" | jq -r '.active_caches_size_in_bytes // ""')
fi

append_marker_matches() {
  local marker="$1"
  local job="$2"
  local pattern="$3"
  local file="$4"
  local lines

  lines=$(grep -Ein "$pattern" "$file" || true)
  [ -n "$lines" ] || return 0

  printf '%s\n' "$lines" | while IFS= read -r line; do
    printf '%s\t%s\t%s\n' "$marker" "$job" "$line" >> "$log_markers_file"
  done
}

scan_logs_file() {
  local name="$1"
  local logs_file="$2"
  local hits misses restores failures downloads builds source_tools sccache_issues sccache_low_utility prepared_workspace
  local normalized_logs_file="${logs_file}.normalized"

  perl -pe 's/\e\[[0-9;]*[A-Za-z]//g; s/^[0-9]{4}-[0-9T:.-]+Z[[:space:]]+//; s/^[^\t]+\t[^\t]+\t[0-9]{4}-[0-9T:.-]+Z[[:space:]]+//' \
    "$logs_file" > "$normalized_logs_file"

  hits=$(grep -Eci 'Cache hit for:|Restored from cache key|Cache restored successfully' "$normalized_logs_file" || true)
  misses=$(grep -Eci "$cache_miss_pattern" "$normalized_logs_file" || true)
  restores=$(grep -Eci 'Restoring cache|Cache Size:' "$normalized_logs_file" || true)
  failures=$(grep -Eci "$cache_failure_pattern" "$normalized_logs_file" || true)
  downloads=$(grep -Eci "$dependency_download_pattern" "$normalized_logs_file" || true)
  builds=$(grep -Eci "$third_party_build_pattern" "$normalized_logs_file" || true)
  source_tools=$(grep -Eci "$source_tool_compile_pattern" "$normalized_logs_file" || true)
  sccache_issues=$(grep -Eci "$sccache_issue_pattern" "$normalized_logs_file" || true)
  sccache_low_utility=$(grep -Eci "$sccache_low_utility_pattern" "$normalized_logs_file" || true)
  prepared_workspace=$(grep -Eci "$prepared_workspace_pattern" "$normalized_logs_file" || true)
  cache_hit_count=$((cache_hit_count + hits))
  cache_miss_count=$((cache_miss_count + misses))
  cache_restore_count=$((cache_restore_count + restores))
  cache_failure_count=$((cache_failure_count + failures))
  dependency_download_count=$((dependency_download_count + downloads))
  third_party_build_count=$((third_party_build_count + builds))
  source_tool_compile_count=$((source_tool_compile_count + source_tools))
  sccache_issue_count=$((sccache_issue_count + sccache_issues))
  sccache_low_utility_count=$((sccache_low_utility_count + sccache_low_utility))
  prepared_workspace_count=$((prepared_workspace_count + prepared_workspace))

  append_marker_matches "cache miss" "$name" "$cache_miss_pattern" "$normalized_logs_file"
  append_marker_matches "cache failure" "$name" "$cache_failure_pattern" "$normalized_logs_file"
  append_marker_matches "dependency download" "$name" "$dependency_download_pattern" "$normalized_logs_file"
  append_marker_matches "third-party compile/check/build" "$name" "$third_party_build_pattern" "$normalized_logs_file"
  append_marker_matches "source tool compile" "$name" "$source_tool_compile_pattern" "$normalized_logs_file"
  append_marker_matches "sccache issue" "$name" "$sccache_issue_pattern" "$normalized_logs_file"
  append_marker_matches "sccache low utility" "$name" "$sccache_low_utility_pattern" "$normalized_logs_file"
  append_marker_matches "prepared workspace" "$name" "$prepared_workspace_pattern" "$normalized_logs_file"
  return 0
}

while IFS= read -r job_b64; do
  [ -n "$job_b64" ] || continue
  job_json=$(printf '%s' "$job_b64" | base64 --decode)
  job_id=$(printf '%s' "$job_json" | jq -r '.databaseId // .id')
  name=$(printf '%s' "$job_json" | jq -r '.name')
  status=$(printf '%s' "$job_json" | jq -r '.status')
  conclusion=$(printf '%s' "$job_json" | jq -r '.conclusion // ""')
  url=$(printf '%s' "$job_json" | jq -r '.html_url // .url')
  started_at=$(printf '%s' "$job_json" | jq -r '.startedAt // .started_at // ""')
  completed_at=$(printf '%s' "$job_json" | jq -r '.completedAt // .completed_at // ""')
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
    | {
        started_at: (.startedAt // .started_at // ""),
        completed_at: (.completedAt // .completed_at // ""),
        name,
        conclusion: (.conclusion // "")
      }
    | select(.started_at != "" and .completed_at != "" and (.completed_at | startswith("0001-") | not))
    | [.started_at, .completed_at, $job, .name, .conclusion]
    | @tsv
  ' | while IFS=$'\t' read -r step_start step_end step_job step_name step_conclusion; do
    step_start_s=$(epoch "$step_start")
    step_end_s=$(epoch "$step_end")
    if [ "$step_start_s" -gt 0 ] && [ "$step_end_s" -gt "$step_start_s" ]; then
      printf '%s\t%s\t%s\t%s\n' \
        "$((step_end_s - step_start_s))" "$step_job" "$step_name" "$step_conclusion" >> "$steps_file"
    fi
  done

  logs_file="${RUNNER_TEMP:-/tmp}/workflow-job-${run_id}-${job_id}.log"
  if gh api "repos/${repo}/actions/jobs/${job_id}/logs" > "$logs_file" 2>/dev/null; then
    scan_logs_file "$name" "$logs_file"
  else
    printf 'log unavailable: %s (%s)\n' "$name" "$job_id" >> "$cache_file"
  fi
done < "$jobs_file"

artifacts_file="${RUNNER_TEMP:-/tmp}/workflow-artifacts-${run_id}.tsv"
if gh api "repos/${repo}/actions/runs/${run_id}/artifacts?per_page=100" \
  --paginate \
  --jq '.artifacts[] | select(.name == "job-log") | [.id, .size_in_bytes] | @tsv' > "$artifacts_file" 2>/dev/null; then
  while IFS=$'\t' read -r artifact_id artifact_size; do
    [ -n "$artifact_id" ] || continue
    artifact_zip="${RUNNER_TEMP:-/tmp}/workflow-job-log-${run_id}-${artifact_id}.zip"
    artifact_log="${RUNNER_TEMP:-/tmp}/workflow-job-log-${run_id}-${artifact_id}.log"
    if gh api "repos/${repo}/actions/artifacts/${artifact_id}/zip" > "$artifact_zip" 2>/dev/null &&
      unzip -p "$artifact_zip" job-log.txt > "$artifact_log" 2>/dev/null; then
      velnor_job_log_artifact_count=$((velnor_job_log_artifact_count + 1))
      scan_logs_file "Velnor job-log artifact ${artifact_id}" "$artifact_log"
    else
      printf 'job-log artifact unavailable: %s (%s bytes)\n' "$artifact_id" "${artifact_size:-unknown}" >> "$cache_file"
    fi
  done < "$artifacts_file"
else
  printf 'job-log artifact list unavailable for run %s\n' "$run_id" >> "$cache_file"
fi

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
    Hygiene)
      print_elapsed_metric "Time to hygiene complete" "$(job_completion_metric '^Cache usage review$|^Scheduled hygiene$|^Native macOS smoke$')"
      ;;
    jackin-dev)
      print_elapsed_metric "Time to jackin-dev published" "$(job_completion_metric '^publish$')"
      print_elapsed_metric "Time to jackin-dev builds complete" "$(job_completion_metric '^build .*$')"
      ;;
    Release)
      print_elapsed_metric "Time to GitHub release published" "$(job_completion_metric '^release$')"
      print_elapsed_metric "Time to release pipeline complete" "$(job_completion_metric '^homebrew$|^release$')"
      ;;
    Renovate)
      print_elapsed_metric "Time to Renovate complete" "$(job_completion_metric '^renovate$')"
      ;;
    "Renovate Validate")
      print_elapsed_metric "Time to Renovate validate complete" "$(job_completion_metric '^validate$')"
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
      if (step ~ /cargo |Cargo |nextest|package tests|clippy|fmt|audit|deny|shear|fuzz|bench|schema/) return "cargo"
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

print_lane_totals() {
  printf '\n### Runner lane totals\n\n'
  printf '| Lane | Aggregate job time | Jobs | Longest job |\n| --- | ---: | ---: | --- |\n'

  awk -F '\t' '
    function lane_for(name) {
      if (name ~ /\(GitHub\)$/) return "GitHub"
      if (name ~ /\(Velnor\)$/) return "Velnor"
      return "shared"
    }
    {
      lane = lane_for($2)
      seconds[lane] += $1
      count[lane] += 1
      if ($1 > max[lane]) {
        max[lane] = $1
        max_name[lane] = $2
      }
    }
    END {
      for (lane in seconds) {
        printf "%d\t%s\t%d\t%d\t%s\n", seconds[lane], lane, count[lane], max[lane], max_name[lane]
      }
    }
  ' "$durations_file" | sort -rn | while IFS=$'\t' read -r seconds lane count max_seconds max_name; do
    printf '| %s | %s | %s | %s (%s) |\n' \
      "$lane" "$(duration "$seconds")" "$count" "$max_name" "$(duration "$max_seconds")"
  done
}

print_github_cache_budget() {
  printf '\n### GitHub Actions Cache Budget\n\n'

  if [ -z "$github_cache_count" ] || [ -z "$github_cache_bytes" ]; then
    printf -- '- Cache usage: unavailable; actions cache API did not return usage for this token.\n'
    return
  fi

  awk -v count="$github_cache_count" -v bytes="$github_cache_bytes" 'BEGIN {
    gib = bytes / 1024 / 1024 / 1024
    pct = gib / 10 * 100
    printf "- Active caches: %s\n", count
    printf "- Active cache size: %.2f GiB of the 10 GiB repository budget reference (%.1f%%)\n", gib, pct
  }'
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
  printf -- '- Rebuild/download markers in job logs: %s dependency download markers, %s third-party compile/check/build markers, %s source-tool compile markers, %s sccache issue markers, %s sccache low-utility markers, %s prepared-workspace artifact markers\n\n' \
    "$dependency_download_count" "$third_party_build_count" "$source_tool_compile_count" "$sccache_issue_count" "$sccache_low_utility_count" "$prepared_workspace_count"
  printf -- '- Velnor job-log artifacts scanned: %s\n\n' "$velnor_job_log_artifact_count"

  print_target_metrics
  print_github_cache_budget

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
  print_lane_totals

  if [ -s "$cache_file" ]; then
    printf '\n### Notes\n\n'
    sed 's/^/- /' "$cache_file"
  fi

  if [ -s "$log_markers_file" ]; then
    printf '\n### Cache, Rebuild, and Download Markers\n\n'
    printf 'These markers require review on warm sequential runs. True first-run cache population is acceptable; repeated cache misses/failures, dependency downloads, source tool compiles, third-party compile/check/build markers, low compiler-cache utility, or prepared-workspace artifact restores should be explained or fixed.\n\n'
    printf '| Marker | Job | Log line |\n| --- | --- | --- |\n'
    head -n 25 "$log_markers_file" | while IFS=$'\t' read -r marker job line; do
      line="${line//|/\\|}"
      printf "| %s | %s | \`%s\` |\n" "$marker" "$job" "$line"
    done
    marker_total=$(wc -l < "$log_markers_file" | tr -d ' ')
    if [ "$marker_total" -gt 25 ]; then
      printf '\nShowing first 25 of %s markers.\n' "$marker_total"
    fi
  fi
} >> "$summary"
