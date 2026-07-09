# AGENTS.md — jackin-host

Host OS integration: desktop notifications, clipboard, caffeinate/keep-awake.

## Rules (this crate)

- Contain platform differences here: macOS/Linux/Windows specifics for clipboard, notifications, and keep-awake live in this crate — do not leak `#[cfg(target_os)]` branches into higher crates.
- Keep-awake is session-scoped: tie caffeinate to active session lifetime and release on teardown; never leave a global inhibit behind.

Workspace rules: [../AGENTS.md](../AGENTS.md). Repo rules: [../../AGENTS.md](../../AGENTS.md).
