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

if [ "$all_features" = true ]; then
  version=v5
  latest_version=latest-v5
  legacy_version=v4
  legacy_latest_version=latest-v4
else
  version=default-v5
  latest_version=default-latest-v5
  legacy_version=default-v4
  legacy_latest_version=default-latest-v4
fi

artifact_prefix="ci-crate-target-${version}-${runner_os}-${runner_arch}-${package}-${cache_key}"
latest_marker_name="ci-crate-target-${latest_version}-${runner_os}-${runner_arch}-${package}"
latest_prefix_base="ci-crate-target-${version}-${runner_os}-${runner_arch}-${package}-"
legacy_prefix="ci-crate-target-${legacy_version}-${runner_os}-${runner_arch}-${package}-${cache_key}"
legacy_marker_name="ci-crate-target-${legacy_latest_version}-${runner_os}-${runner_arch}-${package}-ready"
legacy_prefix_base="ci-crate-target-${legacy_version}-${runner_os}-${runner_arch}-${package}-"

hit=false
canonical_hit=false
format=none
download_prefix=
legacy_name=
run_id=

artifact_run() {
  local name=$1
  gh api "repos/${repository}/actions/artifacts?name=${name}&per_page=10" \
    --jq '.artifacts[] | select(.expired == false) | .workflow_run.id' | head -n 1
}

read_pointer() {
  local marker_name=$1
  local marker_dir="target/.ci-latest-marker-${package}"
  local artifact_id
  artifact_id=$(gh api "repos/${repository}/actions/artifacts?name=${marker_name}&per_page=10" \
    --jq '.artifacts[] | select(.expired == false) | .id' | head -n 1)
  [ -n "$artifact_id" ] || return 1
  rm -rf "$marker_dir"
  mkdir -p "$marker_dir"
  gh api "repos/${repository}/actions/artifacts/${artifact_id}/zip" > "$marker_dir/artifact.zip"
  python3 -m zipfile -e "$marker_dir/artifact.zip" "$marker_dir"
  cat "$marker_dir/target.latest"
  rm -rf "$marker_dir"
}

run_id=$(artifact_run "$artifact_prefix")
if [ -n "$run_id" ]; then
  hit=true
  canonical_hit=true
  format=v5
  download_prefix=$artifact_prefix
else
  # Temporary read compatibility seeds the first single-archive producer from
  # v4. Delete this branch once v5 artifacts exist on main.
  run_id=$(artifact_run "${legacy_prefix}-ready")
  if [ -n "$run_id" ]; then
    hit=true
    format=v4
    download_prefix=$legacy_prefix
  fi
fi

if [ "$hit" != true ]; then
  candidate=$(read_pointer "$latest_marker_name" || true)
  if [ -n "$candidate" ]; then
    if [[ ! "$candidate" =~ ^${latest_prefix_base}[0-9a-f]{64}$ ]]; then
      echo "invalid latest-target prefix for ${package}" >&2
      exit 1
    fi
    run_id=$(artifact_run "$candidate")
    if [ -n "$run_id" ]; then
      hit=true
      format=v5
      download_prefix=$candidate
    fi
  fi
fi

if [ "$hit" != true ]; then
  candidate=$(read_pointer "$legacy_marker_name" || true)
  if [ -n "$candidate" ]; then
    if [[ ! "$candidate" =~ ^${legacy_prefix_base}[0-9a-f]{64}$ ]]; then
      echo "invalid legacy latest-target prefix for ${package}" >&2
      exit 1
    fi
    run_id=$(artifact_run "${candidate}-ready")
    if [ -n "$run_id" ]; then
      hit=true
      format=v4
      download_prefix=$candidate
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
