# Plan 047: Build the host-daemon lifecycle foundation

> **Executor instructions**: Productionize only the empty host-daemon foundation
> scoped by Plan 042. Do not add live-auth-sync, host-bridge, keep-awake
> migration, or production attention notifications in this plan. Update
> `plans/README.md` when done.

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: MED
- **Depends on**: 042
- **Category**: direction (DIRECTION-02)
- **Planned at**: Plan 042 spike, 2026-07-04
- **Completed**: Plan 047 shipped the empty production daemon lifecycle in
  `crates/jackin-runtime/src/host_daemon.rs`, wired through `jackin daemon ...`. It intentionally
  reports no enabled adapters.

## Why this matters

Reactive features need one reviewed host daemon shape before adapters start
carrying credentials or host actions. Plan 042 resolved the lifecycle, install,
control-socket, security, redaction, and host-vs-Capsule boundaries; this plan
turns that foundation into a production CLI surface with no watchers.

## Scope

In scope:

- `jackin daemon serve`
- `jackin daemon install` / `uninstall`
- `jackin daemon start` / `stop` / `restart`
- `jackin daemon status` / `logs`
- per-user launchd LaunchAgent and systemd user unit writers
- Unix socket at `~/.jackin/run/jackin-daemon.sock` under a `0700` directory
- JSONL `{ id, protocol_version, type, ... }` request/response protocol
- daemon/CLI build-id and protocol-version skew failure
- secret-redacted logs and coredump-disable best effort before adapters can hold secrets

Out of scope:

- attention OS notifications (Plan 048)
- live-auth-sync
- host bridge
- keep-awake migration
- Desktop Agent Hub endpoints

## Steps

1. Add the `daemon` CLI subcommand group and route lifecycle commands without
   auto-starting from unrelated commands.
2. Move the Plan 042 prototype protocol into production modules, keeping
   `daemon-spike` test coverage until the production modules have equivalent
   tests.
3. Implement the foreground `serve` loop with `hello`, `status`, and clean
   shutdown requests only.
4. Add launchd/systemd user-unit writers that require explicit operator action;
   package install must not silently write host services.
5. Add same-UID socket permissions, stale-socket cleanup, protocol/build skew
   errors, and bounded request sizes.
6. Add log redaction and coredump-disable best effort. If coredump disable is
   unsupported on a platform, status must report the residual risk.
7. Update daemon, install, troubleshooting, and roadmap docs in the same PR.

## Done criteria

- [x] `jackin daemon serve/start/stop/restart/status/logs/install/uninstall` are implemented and documented
- [x] socket directory is `0700`, protocol/build skew fails closed, and request size is bounded
- [x] daemon logs use shared redaction and status reports coredump policy
- [x] no reactive adapter is enabled by this plan
- [x] focused lifecycle/protocol tests pass on macOS/Linux-compatible code paths
- [x] docs and roadmap are updated
- [x] `plans/README.md` row updated

## Stop conditions

- If same-UID socket auth is not enough for even `hello`/`status`, stop and
  design the extra auth layer before writing adapters.
- If launchd/systemd unit writes require host paths outside jackin-owned config
  roots, surface the host-write opt-in in command output and docs before writing.

## Deferred by design

`live-auth-sync` and `host-bridge` remain blocked after this plan until the
foundation is shipped and reviewed. They carry credentials or host actions, so
they must not be used to prove daemon lifecycle basics.
