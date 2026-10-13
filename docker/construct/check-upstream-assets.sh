#!/usr/bin/env bash
set -euo pipefail

. ./docker/construct/versions.env
if [ -z "${MISE_VERSION:-}" ]; then
  echo "::error::MISE_VERSION is empty" >&2
  exit 1
fi

for arch in x64 arm64; do
  url="https://github.com/jdx/mise/releases/download/v${MISE_VERSION}/mise-v${MISE_VERSION}-linux-${arch}.tar.gz"
  if ! curl --fail --silent --show-error --location --head \
    --retry 5 --retry-all-errors --retry-delay 2 \
    --connect-timeout 15 --max-time 60 \
    -o /dev/null "$url"; then
    echo "::error::${url} did not resolve; pinned release is missing linux-${arch}" >&2
    exit 1
  fi
done
