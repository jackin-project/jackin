#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Alexey Zhokhov
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

if [ "$EVENT_NAME" = "workflow_dispatch" ]; then
  selection=(--all)
elif [ -n "$BASE_REF" ]; then
  git fetch --no-tags --depth=1 origin "$BASE_REF:refs/remotes/origin/$BASE_REF"
  selection=(--base "origin/$BASE_REF")
elif [ -n "$BEFORE_SHA" ] && ! [[ "$BEFORE_SHA" =~ ^0+$ ]]; then
  git fetch --no-tags --depth=1 origin "$BEFORE_SHA"
  selection=(--base "$BEFORE_SHA")
else
  selection=(--all)
fi

selected=$("$CI_XTASK" affected-crates --metadata "$CI_METADATA" "${selection[@]}")
cache_keys=$("$CI_XTASK" affected-crates \
  --metadata "$CI_METADATA" "${selection[@]}" --cache-keys)
misses=()
hits=()
while IFS= read -r package; do
  cache_key=$(jq -er --arg package "$package" '.[$package]' <<< "$cache_keys")
  result=$(scripts/ci/find-crate-result.sh \
    "$package" "$cache_key" true \
    "$DOCKER_E2E" "$CONSTRUCT_IMAGE_CHANGED" \
    "$COMMON_CONTRACT_KEY" "$DOCKER_CONTRACT_KEY" \
    Linux X64 "$SOURCE_SHA")
  if [ "$(jq -r '.hit' <<< "$result")" = "true" ]; then
    hits+=("$package")
  else
    misses+=("$package")
  fi
done < <(jq -r '.[]' <<< "$selected")

packages=$(printf '%s\n' "${misses[@]}" | jq -Rsc 'split("\n") | map(select(length > 0))')
reused=$(printf '%s\n' "${hits[@]}" | jq -Rsc 'split("\n") | map(select(length > 0))')
jq -cn \
  --argjson packages "$packages" \
  --argjson cache_keys "$cache_keys" \
  --argjson reused_packages "$reused" \
  '{packages: $packages, cache_keys: $cache_keys, reused_packages: $reused_packages}'
