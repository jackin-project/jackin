# jackin-runtime-session-control

Host client for capsule
`session.send` and `events`.

## What this crate owns

- Send (`session_control`):
  `send_session_text`,
  `control_socket_path`,
  `ControlTransport`.
- Events (`session_control`):
  `SessionEvents`, event
  watch + exit mapping.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`session_control.rs`](src/session_control.rs) | send + events | `session_control/tests/` |
| [`session_control/`](src/session_control/) | test suites | `session_control/tests/` |

## Public API

`session_control::send_session_text`,
`session_control::SessionEvents`,
`session_control::ControlTransport`,
`session_control::control_socket_path`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-session-control
cargo clippy -p jackin-runtime-session-control --all-targets -- -D warnings
```
