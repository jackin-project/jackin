#!/bin/bash
set -euo pipefail

# Sourced into the entrypoint shell so the export reaches the launched
# agent. preflight is a child process — its exports do not propagate.
#
# Gradle reads POSTGRESQL_DB_HOST to reach the docker-compose Postgres.
if [ -n "${JACKIN_DIND_HOSTNAME:-}" ]; then
    export POSTGRESQL_DB_HOST="$JACKIN_DIND_HOSTNAME"
fi
