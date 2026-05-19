#!/bin/bash
# Container supervisor — PID 1.
#
# Exits 0 when the last tmux session ends so host-side cleanup fires
# automatically — no manual `jackin eject` needed. Exits 1 if no session
# appears within the startup grace period so diagnose_premature_exit can
# surface the container logs.
#
# The tmux server creates its socket at /tmp/tmux-<uid>/default when the
# first session starts. The grace period watches for the socket file to
# appear; the monitor loop polls via `tmux list-sessions` rather than a
# plain file-existence check so a stale socket (tmux crashed without
# cleanup) doesn't keep the supervisor alive indefinitely.
#
# Will be removed in Phase 2 when the `jackin-container` Rust binary takes
# over as PID 1 with inotify-based socket watching.
# See reference/roadmap/jackin-container-binary for the full plan.
#
# No `set -e`: signal-killed `wait` exits non-zero; `set -e` would misread
# that as a supervisor failure on every clean `docker stop`.

_cleanup() {
    kill "$!" 2>/dev/null || true
    exit 0
}
trap '_cleanup' TERM INT

TMUX_SOCKET="/tmp/tmux-$(id -u)/default"

# Grace period: wait up to 60 s for the first tmux session socket to
# appear. Without this the supervisor exits before `docker exec tmux
# new-session` has a chance to create it.
deadline=$((SECONDS + 60))
while [ $SECONDS -lt $deadline ] && [ ! -S "$TMUX_SOCKET" ]; do
    sleep 1 &
    wait $! || true
done

# No session appeared — something went wrong at startup. Exit non-zero so
# diagnose_premature_exit surfaces the container logs rather than returning
# a cryptic "container is not running" error.
if [ ! -S "$TMUX_SOCKET" ]; then
    echo "supervisor: no tmux socket at ${TMUX_SOCKET} after 60 s; is tmux installed and starting correctly?" >&2
    exit 1
fi

# Wait for the last session to end. Poll via `tmux list-sessions` rather
# than a plain socket-file existence check: if tmux crashes or is killed
# without removing the socket, a stale file would keep the supervisor
# alive indefinitely.
while tmux list-sessions &>/dev/null; do
    sleep 1 &
    wait $! || true
done

exit 0
