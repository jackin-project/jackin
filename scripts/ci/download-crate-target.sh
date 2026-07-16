#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Alexey Zhokhov
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

run_id=$1
download_prefix=$2
destination=$3
repository=${REPOSITORY:-${GITHUB_REPOSITORY:?GITHUB_REPOSITORY must be set}}

rm -rf "$destination"
mkdir -p "$destination"
mapfile -t artifacts < <(
  gh api "repos/${repository}/actions/runs/${run_id}/artifacts" --paginate \
    --jq '.artifacts[] | select(.expired == false) | [.id, .name] | @tsv'
)

selected=
for artifact in "${artifacts[@]}"; do
  read -r artifact_id artifact_name <<< "$artifact"
  if [ "$artifact_name" = "$download_prefix" ]; then
    selected="${artifact_id}"$'\t'"${artifact_name}"
    break
  fi
done

if [ -z "$selected" ]; then
  echo "no target artifact found for ${download_prefix} in run ${run_id}" >&2
  exit 1
fi

read -r artifact_id artifact_name <<< "$selected"
archive="${destination}/${artifact_name}.zip"
gh api "repos/${repository}/actions/artifacts/${artifact_id}/zip" > "$archive"
python3 -m zipfile -e "$archive" "$destination"
rm -f "$archive"
