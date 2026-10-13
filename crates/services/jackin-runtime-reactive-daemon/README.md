# jackin-runtime-reactive-daemon

Feature-gated host-daemon
control-socket spike.

## What this crate owns

- Socket (`reactive_daemon`):
  `bind_control_socket`,
  `serve_one`,
  `handle_connection` —
  single-line JSON RPC.
- Adapter (`reactive_daemon`):
  `AttentionAdapter`,
  `AttentionNotifier` —
  blocked/done edges from
  status snapshots.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | gated re-export | — |
| [`reactive_daemon.rs`](src/reactive_daemon.rs) | socket + adapter | `reactive_daemon/tests.rs` |
| [`reactive_daemon/`](src/reactive_daemon/) | test suites | `reactive_daemon/tests.rs` |

## Public API

`reactive_daemon::serve_one`,
`reactive_daemon::AttentionAdapter`,
`reactive_daemon::DaemonRequest`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-reactive-daemon --features daemon-spike
cargo clippy -p jackin-runtime-reactive-daemon --all-targets --features daemon-spike -- -D warnings
```
