# jackin-runtime-exec-host

Host-side credential resolver
for `jackin-exec`: a Unix-socket
listener that resolves on-demand
env vars for role containers.

## What this crate owns

- Listener (`exec_host`):
  `start`, `start_for_container`,
  `start_bound_for_container`,
  request validation against the
  `allowed_bindings` set.
- Auth (`exec_host`):
  `ensure_caller_auth_supported`
  peer-credential preflight.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`exec_host.rs`](src/exec_host.rs) | listener + API root | `exec_host/tests/` |
| [`exec_host/`](src/exec_host/) | test suites | `exec_host/tests/` |

## Public API

`exec_host::start`,
`exec_host::start_for_container`,
`exec_host::start_bound_for_container`,
`exec_host::ensure_caller_auth_supported`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-exec-host
cargo clippy -p jackin-runtime-exec-host --all-targets -- -D warnings
```
