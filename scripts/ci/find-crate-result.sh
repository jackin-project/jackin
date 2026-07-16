#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Alexey Zhokhov
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

package=$1
cache_key=$2
all_features=$3
docker_e2e=$4
construct_image_changed=$5
common_contract_key=$6
docker_contract_key=$7
runner_os=$8
runner_arch=$9
source_sha=${10}

contract_key=$common_contract_key
if [ "$package" = "jackin" ]; then
  contract_key=$(printf '%s' "${common_contract_key}:${docker_contract_key}" \
    | sha256sum | cut -d ' ' -f 1)
else
  docker_e2e=false
  construct_image_changed=false
fi

suffix="af${all_features}-e2e${docker_e2e}-construct${construct_image_changed}"
sha_name="ci-crate-result-sha-v1-${runner_os}-${runner_arch}-${package}-${source_sha}-${contract_key}-${suffix}"
name=$sha_name
artifact_id=

lookup_artifact() {
  gh api "repos/${GITHUB_REPOSITORY}/actions/artifacts?name=$1&per_page=10" \
    --jq '.artifacts[] | select(.expired == false) | .id' \
    | head -n 1
}

if [ -n "$cache_key" ]; then
  name="ci-crate-result-v1-${runner_os}-${runner_arch}-${package}-${cache_key}-${contract_key}-${suffix}"
  if ! artifact_id=$(lookup_artifact "$name"); then
    echo "::warning::successful-result lookup failed; scheduling ${package}" >&2
    artifact_id=
  fi
fi
if [ -z "$artifact_id" ]; then
  if ! artifact_id=$(lookup_artifact "$sha_name"); then
    echo "::warning::successful-result SHA lookup failed; scheduling ${package}" >&2
    artifact_id=
  fi
fi

jq -cn \
  --arg name "$name" \
  --arg sha_name "$sha_name" \
  --arg artifact_id "$artifact_id" \
  '{name: $name, sha_name: $sha_name, artifact_id: $artifact_id, hit: ($artifact_id != "")}'
