#!/bin/bash
# Container supervisor — PID 1.
#
# Keeps the container alive while agent sessions run via `docker exec`.
# Forwards SIGTERM and SIGINT so `docker stop` / `docker kill` terminate
# cleanly without a 10-second timeout.
set -euo pipefail

_cleanup() {
    exit 0
}
trap '_cleanup' TERM INT

# Wait in a background-sleep loop so the trap fires promptly.
while true; do
    sleep 3600 &
    wait $!
done
