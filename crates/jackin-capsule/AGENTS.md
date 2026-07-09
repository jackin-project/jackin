# AGENTS.md — jackin-capsule

The in-container binary: PID-1 re-emitting PTY multiplexer daemon, plus the host-side client that attaches to it. Owns the capsule control plane — sessions, attach/displace, status, clipboard, git/PR watch, token totals, control-protocol routing.

## Hard rules (this crate)

- **Tier & dependencies:** L4 entry/glue crate. Allowed workspace deps: `jackin-agent-status`, `jackin-core`, `jackin-diagnostics`, `jackin-protocol`, `jackin-usage`, `jackin-term`, `jackin-tui`, `jackin-build-meta`. Must NOT depend on host-side runtime (`jackin-runtime`) or other host-only crates.
- **Keep `README.md` current:** update it when structure, public API, module layout, daemon subsystems, or the control protocol change (see `crates/AGENTS.md`).
- **Wire format is shared.** Control/attach/protocol types live in `jackin-protocol`; a wire change aligns host + capsule in the same PR.
- **`daemon.rs` decomposes, not grows.** Reduce the daemon into an actor/event-loop shell plus owned subsystems; split production responsibility first, then move each subsystem's tests with it. Do not split `tests.rs` to cut line count.
- **No blocking on the render/control path.** Blocking process/filesystem work goes through async helpers or `spawn_blocking` (enforced by `clippy::disallowed_methods`); the terminal emit path stays allocation-lean.

## What lives here vs elsewhere

- This crate owns: the capsule daemon, attach/client, sessions, PTY ownership, status publication, clipboard, git/PR watch, sudo provisioning, PID-1 setup, MCP server.
- The terminal *model* it drives lives in `jackin-term`. Wire contracts live in `jackin-protocol`. Agent-status authority lives in `jackin-agent-status`. Usage/telemetry substrate lives in `jackin-usage`.

Workspace rules: [../AGENTS.md](../AGENTS.md). Repo rules: [../../AGENTS.md](../../AGENTS.md).
