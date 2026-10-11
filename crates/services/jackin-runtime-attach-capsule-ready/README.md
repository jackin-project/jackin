# jackin-runtime-attach-capsule-ready

Capsule daemon
readiness waits.

## What this crate owns

- Readiness
  (`capsule_ready`):
  `wait_for_capsule_daemon_with_handle`,
  `wait_for_capsule_daemon_ready`,
  `capsule_daemon_socket_connects`,
  `capsule_socket_negotiates` —
  socket + protocol
  waits.
- Warmup
  (`capsule_ready`):
  `wait_for_dind` —
  DinD sidecar ready +
  TLS cert wait.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`capsule_ready.rs`](src/capsule_ready.rs) | readiness + warmup | hub `attach` suite |

## Public API

`capsule_ready::wait_for_capsule_daemon_with_handle`,
`capsule_ready::wait_for_capsule_daemon_ready`,
`capsule_ready::capsule_daemon_socket_connects`,
`capsule_ready::capsule_socket_negotiates`,
`capsule_ready::wait_for_dind`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-attach-capsule-ready
cargo clippy -p jackin-runtime-attach-capsule-ready --all-targets -- -D warnings
```
