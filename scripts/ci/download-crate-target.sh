#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Alexey Zhokhov
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

run_id=$1
format=$2
download_prefix=$3
legacy_name=$4
destination=$5
repository=${REPOSITORY:-${GITHUB_REPOSITORY:?GITHUB_REPOSITORY must be set}}

rm -rf "$destination"
mkdir -p "$destination"
mapfile -t artifacts < <(
  gh api "repos/${repository}/actions/runs/${run_id}/artifacts" --paginate \
    --jq '.artifacts[] | select(.expired == false) | [.id, .name] | @tsv'
)

selected=()
for artifact in "${artifacts[@]}"; do
  read -r artifact_id artifact_name <<< "$artifact"
  if { [ "$format" = v4 ] || [ "$format" = v3 ]; } && \
    [[ "$artifact_name" == "${download_prefix}-part-"* ]]; then
    selected+=("${artifact_id}"$'\t'"${artifact_name}")
  elif [ "$format" = v2 ] && [ "$artifact_name" = "$legacy_name" ]; then
    selected+=("${artifact_id}"$'\t'"${artifact_name}")
  fi
done

if [ "${#selected[@]}" -eq 0 ]; then
  echo "no target artifacts found for ${download_prefix:-$legacy_name} in run ${run_id}" >&2
  exit 1
fi

pids=()
archives=()
for artifact in "${selected[@]}"; do
  read -r artifact_id artifact_name <<< "$artifact"
  archive="${destination}/${artifact_name}.zip"
  gh api "repos/${repository}/actions/artifacts/${artifact_id}/zip" > "$archive" &
  pids+=("$!")
  archives+=("$archive")
done

failed=0
for pid in "${pids[@]}"; do
  wait "$pid" || failed=1
done
[ "$failed" -eq 0 ]

pids=()
for archive in "${archives[@]}"; do
  python3 -m zipfile -e "$archive" "$destination" &
  pids+=("$!")
done
for pid in "${pids[@]}"; do
  wait "$pid" || failed=1
done
[ "$failed" -eq 0 ]
rm -f "${archives[@]}"
