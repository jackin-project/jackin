#!/bin/bash
set -euo pipefail

# Sourced into the entrypoint shell so the export reaches the launched
# agent. preflight is a child process — its exports do not propagate.
#
# Gradle reads POSTGRESQL_DB_HOST to reach the docker-compose Postgres.
# JACKIN_DIND_HOSTNAME is part of the jackin runtime contract; an unset
# value here means the role is being launched outside jackin and the
# Postgres host cannot be derived. Fail loudly so misconfiguration
# surfaces here, not as a cryptic Gradle connection error later.
if [ -z "${JACKIN_DIND_HOSTNAME:-}" ]; then
    echo "[brown-source] ERROR: JACKIN_DIND_HOSTNAME is required but was not set by jackin" >&2
    return 1
fi
export POSTGRESQL_DB_HOST="$JACKIN_DIND_HOSTNAME"
