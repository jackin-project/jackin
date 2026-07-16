#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Alexey Zhokhov
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

force_package=${FORCE_PACKAGE:-}
if [ -n "$force_package" ]; then
  selection=(--all)
elif [ "$EVENT_NAME" = "workflow_dispatch" ]; then
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
if [ -n "$force_package" ]; then
  jq -e --arg package "$force_package" 'has($package)' <<< "$cache_keys" >/dev/null \
    || { echo "unknown workspace crate: $force_package" >&2; exit 1; }
  selected=$(jq -cn --arg package "$force_package" '[$package]')
fi
misses=()
hits=()
target_results='{}'
while IFS= read -r package; do
  cache_key=$(jq -er --arg package "$package" '.[$package]' <<< "$cache_keys")
  result=$(scripts/ci/find-crate-result.sh \
    "$package" "$cache_key" true \
    "$DOCKER_E2E" "$CONSTRUCT_IMAGE_CHANGED" \
    "$COMMON_CONTRACT_KEY" "$DOCKER_CONTRACT_KEY" \
    Linux X64 "$SOURCE_SHA")
  if [ "$package" != "$force_package" ] && [ "$(jq -r '.hit' <<< "$result")" = "true" ]; then
    hits+=("$package")
  else
    misses+=("$package")
    target_result=$("$CI_XTASK" ci-target find \
      --package "$package" \
      --cache-key "$cache_key" \
      --all-features true \
      --runner-os Linux \
      --runner-arch X64 \
      --repository "$GITHUB_REPOSITORY")
    target_results=$(jq -cn \
      --argjson current "$target_results" \
      --arg package "$package" \
      --argjson target "$target_result" \
      '$current + {($package): $target}')
  fi
done < <(jq -r '.[]' <<< "$selected")

packages=$(printf '%s\n' "${misses[@]}" | jq -Rsc 'split("\n") | map(select(length > 0))')
reused=$(printf '%s\n' "${hits[@]}" | jq -Rsc 'split("\n") | map(select(length > 0))')
jq -cn \
  --argjson packages "$packages" \
  --argjson cache_keys "$cache_keys" \
  --argjson reused_packages "$reused" \
  --argjson target_results "$target_results" \
  '{packages: $packages, cache_keys: $cache_keys, reused_packages: $reused_packages,
    target_results: $target_results}'
