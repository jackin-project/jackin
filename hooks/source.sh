#!/bin/bash
set -euo pipefail

# Export POSTGRESQL_DB_HOST from JACKIN_DIND_HOSTNAME (required for Gradle).
if [ -z "${JACKIN_DIND_HOSTNAME:-}" ]; then
    echo "[brown-source] ERROR: JACKIN_DIND_HOSTNAME is required but was not set by jackin" >&2
    return 1
fi
export POSTGRESQL_DB_HOST="$JACKIN_DIND_HOSTNAME"
