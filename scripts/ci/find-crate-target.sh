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
  version=v6
  legacy_version=v5
else
  version=default-v6
  legacy_version=default-v5
fi

artifact_name="ci-crate-target-${version}-${runner_os}-${runner_arch}-${package}"
legacy_name="ci-crate-target-${legacy_version}-${runner_os}-${runner_arch}-${package}-${cache_key}"
artifact_id=$(
  gh api "repos/${repository}/actions/artifacts?name=${artifact_name}&per_page=10" \
    --jq '[.artifacts[] | select(.expired == false)]
      | sort_by(.created_at) | reverse | .[0].id // empty'
)

hit=false
known_exact=false
if [ -n "$artifact_id" ]; then
  hit=true
else
  artifact_id=$(
    gh api "repos/${repository}/actions/artifacts?name=${legacy_name}&per_page=10" \
      --jq '[.artifacts[] | select(.expired == false)]
        | sort_by(.created_at) | reverse | .[0].id // empty'
  )
  if [ -n "$artifact_id" ]; then
    hit=true
    known_exact=true
  fi
fi

jq -cn \
  --arg name "$artifact_name" \
  --arg source_key "$cache_key" \
  --argjson hit "$hit" \
  --argjson known_exact "$known_exact" \
  --arg artifact_id "$artifact_id" \
  '{name: $name, source_key: $source_key, hit: $hit, known_exact: $known_exact,
    artifact_id: $artifact_id}'
