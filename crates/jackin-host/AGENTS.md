# AGENTS.md — jackin-host

Host OS integration: desktop notifications, clipboard, caffeinate/keep-awake.

## Hard rules (this crate)

- **Tier & dependencies:** L2 infrastructure. Allowed workspace deps: `jackin-core`, `jackin-diagnostics`, `jackin-docker`, `jackin-protocol`, `jackin-tui`. Lower domain crates must not depend on this; presentation reaches host features through it.
- **Keep `README.md` current:** update it when structure, public API, or platform surfaces change (see `crates/AGENTS.md`).
- **Contain platform differences here.** macOS/Linux/Windows specifics for clipboard, notifications, and keep-awake live in this crate; do not leak `#[cfg(target_os)]` platform branches into higher crates.
- **Keep-awake is session-scoped.** Tie caffeinate to active session lifetime; release on teardown. Do not leave a global inhibit behind.

## What lives here vs elsewhere

- This crate owns: keep-awake, clipboard, desktop notifications, host naming, host universe aggregate.
- When keep-awake is *triggered* is decided in `jackin-runtime`/`jackin-console`. Telemetry of host events lives in `jackin-diagnostics`.

Workspace rules: [../AGENTS.md](../AGENTS.md). Repo rules: [../../AGENTS.md](../../AGENTS.md).
