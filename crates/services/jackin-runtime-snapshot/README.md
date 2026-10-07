# jackin-runtime-snapshot

Capsule snapshot and usage
fetch over the control socket.

## What this crate owns

- Fetch (`snapshot`):
  `fetch_snapshot`,
  `fetch_usage_accounts`,
  `socket_path`.
- Transport (`snapshot`):
  `request_control_inner`,
  `run_docker_exec_capsule`,
  `ensure_capsule_protocol_via_docker_exec`.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`snapshot.rs`](src/snapshot.rs) | fetch + transport | `snapshot/tests.rs` |
| [`snapshot/`](src/snapshot/) | test suites | `snapshot/tests.rs` |

## Public API

`snapshot::fetch_snapshot`,
`snapshot::fetch_usage_accounts`,
`snapshot::socket_path`,
`snapshot::InstanceSnapshot`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-snapshot
cargo clippy -p jackin-runtime-snapshot --all-targets -- -D warnings
```
