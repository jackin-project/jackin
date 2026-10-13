# jackin-runtime-host-daemon

Host daemon backend: serves
daemon RPCs to attached sessions
over a Unix-domain socket.
Unix-only.

## What this crate owns

- Server (`host_daemon`):
  `ensure_run_dir`,
  `bind_control_socket`, `serve`,
  `request`,
  `handle_request_line`.
- Units (`host_daemon`):
  `render_unit_files`,
  `install_units`,
  `uninstall_units`, plus the
  notification command builder.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports (unix) | — |
| [`host_daemon.rs`](src/host_daemon.rs) | server + API root | `host_daemon/tests/` |
| [`host_daemon/`](src/host_daemon/) | test suites | `host_daemon/tests/` |

## Public API

`host_daemon::serve`,
`host_daemon::DaemonRequest`,
`host_daemon::DaemonResponse`,
`host_daemon::DaemonLayout`,
`host_daemon::ServeOutcome`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-host-daemon
cargo clippy -p jackin-runtime-host-daemon --all-targets -- -D warnings
```
