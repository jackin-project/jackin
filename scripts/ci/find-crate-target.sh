#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Alexey Zhokhov
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

package=$1
cache_key=$2
all_features=$3
runner_os=$4
runner_arch=$5
repository=${REPOSITORY:-${GITHUB_REPOSITORY:?GITHUB_REPOSITORY must be set}}

if [ "$all_features" = "true" ]; then
  version=v4
  legacy_version=v3
  legacy_archive_version=v2
  latest_version=latest-v4
else
  version=default-v4
  legacy_version=default-v3
  legacy_archive_version=default-v2
  latest_version=default-latest-v4
fi

artifact_prefix="ci-crate-target-${version}-${runner_os}-${runner_arch}-${package}-${cache_key}"
legacy_split_prefix="ci-crate-target-${legacy_version}-${runner_os}-${runner_arch}-${package}-${cache_key}"
legacy_artifact_name="ci-crate-target-${legacy_archive_version}-${runner_os}-${runner_arch}-${package}-${cache_key}"
latest_marker_name="ci-crate-target-${latest_version}-${runner_os}-${runner_arch}-${package}-ready"
latest_prefix_base="ci-crate-target-${version}-${runner_os}-${runner_arch}-${package}-"

hit=false
canonical_hit=false
format=none
download_prefix=
legacy_name=
run_id=$(gh api "repos/${repository}/actions/artifacts?name=${artifact_prefix}-ready&per_page=10" \
  --jq '.artifacts[] | select(.expired == false) | .workflow_run.id' | head -n 1)
if [ -n "$run_id" ]; then
  hit=true
  canonical_hit=true
  format=v4
  download_prefix=$artifact_prefix
else
  run_id=$(gh api "repos/${repository}/actions/artifacts?name=${legacy_split_prefix}-ready&per_page=10" \
    --jq '.artifacts[] | select(.expired == false) | .workflow_run.id' | head -n 1)
  if [ -n "$run_id" ]; then
    hit=true
    format=v3
    download_prefix=$legacy_split_prefix
  else
    run_id=$(gh api "repos/${repository}/actions/artifacts?name=${legacy_artifact_name}&per_page=10" \
      --jq '.artifacts[] | select(.expired == false) | .workflow_run.id' | head -n 1)
    if [ -n "$run_id" ]; then
      hit=true
      format=v2
      legacy_name=$legacy_artifact_name
    fi
  fi
fi

if [ "$hit" != "true" ]; then
  latest_artifact=$(gh api "repos/${repository}/actions/artifacts?name=${latest_marker_name}&per_page=10" \
    --jq '.artifacts[] | select(.expired == false) | [.id, .workflow_run.id] | @tsv' | head -n 1)
  if [ -n "$latest_artifact" ]; then
    read -r latest_artifact_id run_id <<< "$latest_artifact"
    marker_dir="target/.ci-latest-marker-${package}"
    rm -rf "$marker_dir"
    mkdir -p "$marker_dir"
    gh api "repos/${repository}/actions/artifacts/${latest_artifact_id}/zip" > "$marker_dir/artifact.zip"
    python3 -m zipfile -e "$marker_dir/artifact.zip" "$marker_dir"
    download_prefix=$(cat "$marker_dir/target.latest")
    rm -rf "$marker_dir"
    if [[ ! "$download_prefix" =~ ^${latest_prefix_base}[0-9a-f]{64}$ ]]; then
      echo "invalid latest-target prefix for ${package}" >&2
      exit 1
    fi
    run_id=$(gh api "repos/${repository}/actions/artifacts?name=${download_prefix}-ready&per_page=10" \
      --jq '.artifacts | map(select(.expired == false)) | first | .workflow_run.id // empty')
    if [ -n "$run_id" ]; then
      hit=true
      format=v4
    else
      download_prefix=
    fi
  fi
fi

jq -cn \
  --arg prefix "$artifact_prefix" \
  --argjson hit "$hit" \
  --argjson canonical_hit "$canonical_hit" \
  --arg format "$format" \
  --arg download_prefix "$download_prefix" \
  --arg legacy_name "$legacy_name" \
  --arg run_id "$run_id" \
  '{prefix: $prefix, hit: $hit, canonical_hit: $canonical_hit, format: $format,
    download_prefix: $download_prefix, legacy_name: $legacy_name, run_id: $run_id}'
