#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Alexey Zhokhov
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

artifact_id=$1
destination=$2
repository=${REPOSITORY:-${GITHUB_REPOSITORY:?GITHUB_REPOSITORY must be set}}

rm -rf "$destination"
mkdir -p "$destination"
archive="${destination}/artifact.zip"
gh api "repos/${repository}/actions/artifacts/${artifact_id}/zip" > "$archive"
python3 -m zipfile -e "$archive" "$destination"
rm -f "$archive"
