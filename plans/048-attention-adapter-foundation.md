# Plan 048: Build the first production attention adapter

> **Executor instructions**: Promote the Plan 042 attention prototype only after
> Plan 047 lands. Keep the adapter one-way: it consumes status authority events
> and dispatches notifications. Do not add live-auth-sync or host-bridge in this
> plan. Update `plans/README.md` when done.

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: MED
- **Depends on**: 047
- **Category**: direction (DIRECTION-02)
- **Planned at**: Plan 042 spike, 2026-07-04
- **Completed**: Plan 048 promoted the spike's one-way snapshot consumer into the production
  `host_daemon` module. The adapter de-duplicates `blocked` / `done` transitions and is muted by
  default unless the daemon is started with `JACKIN_ATTENTION=1`.

## Why this matters

Idle wall-clock waiting is the clearest operator pain the daemon can address
without carrying secrets or host-command authority. The existing runtime status
authority already computes `blocked` / `done` / `working` / `idle` / `unknown`;
the daemon adapter should consume that authority rather than inventing a second
heuristic.

## Scope

In scope:

- daemon adapter that subscribes to Capsule status snapshots/events
- `blocked` and unseen `done` transition notifications
- macOS Notification Center path and Linux `notify-send` path
- quiet diagnostics-log notifier for unsupported hosts
- per-workspace mute/enable config if the config shape is small; otherwise
  write the config follow-up before enabling defaults
- tests for transition de-duplication, mute behavior, and unsupported notifier fallback

Out of scope:

- `jackin-attention` MCP enrichment server
- click-to-focus
- sound escalation
- console Notifications tab
- live-auth-sync and host bridge

## Steps

1. Replace the `daemon-spike` adapter with production daemon adapter modules
   that consume the daemon event/subscription API from Plan 047.
2. Preserve the Plan 042 transition behavior: notify on first `blocked` and
   first unseen `done`; do not re-notify while the pane remains in the same
   attention state.
3. Add notification backends:
   - macOS: Notification Center command or native wrapper, no secrets in body
   - Linux: `notify-send` when available
   - fallback: daemon diagnostics-log event only
4. Add minimal operator controls. Default should be conservative until the UX is
   validated; mute/disable must be available before any noisy default.
5. Update the roadmap and user docs with exact platform support and residual
   limitations.

## Done criteria

- [x] adapter runs under the production daemon foundation from Plan 047
- [x] notifications fire only on `blocked`/`done` edges and de-duplicate repeated snapshots
- [x] macOS, Linux, and fallback notifier paths are covered by tests or command-runner fakes
- [x] live-auth-sync and host bridge remain unimplemented and documented as deferred
- [x] roadmap/docs describe shipped vs deferred pieces
- [x] `plans/README.md` row updated

## Stop conditions

- If the adapter needs bidirectional state or host approvals to be useful, stop
  and re-scope; this first adapter must remain one-way.
- If OS notification dispatch cannot avoid secret-bearing titles/bodies, keep
  the adapter disabled and document the redaction gap instead of shipping it.
