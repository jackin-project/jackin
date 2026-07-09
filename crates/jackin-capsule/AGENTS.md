# AGENTS.md — jackin-capsule

The in-container binary: PID-1 re-emitting PTY multiplexer daemon + host attach client.

## Rules (this crate)

- `daemon.rs` decomposes, it does not grow: split production responsibility first, then move each subsystem's tests with it. Do not split `tests.rs` to cut line count.
- No blocking on the render/control path — blocking process/filesystem work goes through async helpers or `spawn_blocking` (also enforced by `clippy::disallowed_methods`).
- A wire-format change is a host↔capsule contract change: align both binaries in the same PR (types live in `jackin-protocol`).

## Boundaries

- The terminal *model* this daemon drives lives in `jackin-term` — capsule never recreates a second terminal model beside it.

Workspace rules: [../AGENTS.md](../AGENTS.md). Repo rules: [../../AGENTS.md](../../AGENTS.md).
