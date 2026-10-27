# jackin-host

Host OS integration for jackin❯: desktop notifications, clipboard, and caffeinate/keep-awake (prevent sleep during active sessions). Platform-specific host surface used by the runtime and console.

## What this crate owns

- Keep-awake / caffeinate (`caffeinate`) so the host does not sleep mid-session.
- Host clipboard (`host_clipboard`) and desktop notifications (`host_desktop`).
- Host-side naming (`naming`) and the host "universe" aggregate (`universe`).

## Architecture tier and allowed dependencies

**L2 infrastructure.** Allowed workspace dependencies: the core ports/types, `jackin-diagnostics`, `jackin-docker`, `jackin-protocol`. Lower domain crates (L0) must not depend on this; presentation crates (L3) reach host-clipboard/desktop through it. Neutral TUI is TermRock, not a workspace `jackin-tui` crate.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | crate root, re-exports | — |
| [`caffeinate.rs`](src/caffeinate.rs) · [`caffeinate/`](src/caffeinate.rs) | keep-awake | — |
| [`host_clipboard.rs`](src/host_clipboard.rs) · [`host_clipboard/`](src/host_clipboard.rs) | clipboard | — |
| [`host_desktop.rs`](src/host_desktop.rs) · [`host_desktop/`](src/host_desktop.rs) | desktop notifications | — |
| [`process_telemetry.rs`](src/process_telemetry.rs) · [`process_telemetry/`](src/process_telemetry.rs) | bounded host subprocess telemetry ownership | — |
| [`naming.rs`](src/naming.rs) | host naming | — |
| [`universe.rs`](src/universe.rs) | host universe aggregate | — |

## Public API

Keep-awake, clipboard, and desktop-notify entry points consumed by `jackin-runtime` and `jackin-console`. Platform differences are contained here, not leaked upward.

## How to verify

```sh
cargo nextest run -p jackin-host --no-tests warn
cargo clippy -p jackin-host --all-targets -- -D warnings
```
