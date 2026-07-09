# jackin-host

Host OS integration for jackin❯: desktop notifications, clipboard, and caffeinate/keep-awake (prevent sleep during active sessions). Platform-specific host surface used by the runtime and console.

## What this crate owns

- Keep-awake / caffeinate (`caffeinate`) so the host does not sleep mid-session.
- Host clipboard (`host_clipboard`) and desktop notifications (`host_desktop`).
- Host-side naming (`naming`) and the host "universe" aggregate (`universe`).

## Architecture tier and allowed dependencies

**L2 infrastructure.** Allowed workspace dependencies: the core ports/types, `jackin-diagnostics`, `jackin-docker`, `jackin-protocol`, `jackin-tui`. Lower domain crates (L0) must not depend on this; presentation crates (L3) reach host-clipboard/desktop through it.

## Structure

- `src/caffeinate.rs` / `src/caffeinate/` — keep-awake
- `src/host_clipboard.rs` / `src/host_clipboard/` — clipboard
- `src/host_desktop.rs` / `src/host_desktop/` — desktop notifications
- `src/naming.rs`, `src/universe.rs` — host naming + aggregate

## Public API

Keep-awake, clipboard, and desktop-notify entry points consumed by `jackin-runtime` and `jackin-console`. Platform differences are contained here, not leaked upward.

## How to verify

```sh
cargo nextest run -p jackin-host
cargo clippy -p jackin-host --all-targets -- -D warnings
```

