#!/bin/bash
# Container supervisor — PID 1.
#
# Keeps the container alive while agent sessions run via `docker exec`.
# Forwards SIGTERM and SIGINT so `docker stop` / `docker kill` terminate
# cleanly without a 10-second timeout.
#
# No `set -e`: `wait` returns the exit code of the child it waited on;
# a signal-killed sleep exits non-zero, and `set -e` would misread that
# as a supervisor failure on every clean `docker stop`.

_cleanup() {
    kill "$!" 2>/dev/null || true
    exit 0
}
trap '_cleanup' TERM INT

# Wait in a background-sleep loop so the trap fires promptly.
# `|| true` guards against a signal-killed sleep triggering an exit.
while true; do
    sleep 3600 &
    wait $! || true
done
