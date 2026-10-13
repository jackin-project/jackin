# jackin-runtime-identity

Host git identity capture
and capsule supervisor user.

## What this crate owns

- Identity (`identity`):
  `GitIdentity`,
  `load_git_identity`,
  `try_capture`,
  `host_uid`.
- Boundary (`identity`):
  `CAPSULE_SUPERVISOR_USER`.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`identity.rs`](src/identity.rs) | capture + const | `identity/tests.rs` |
| [`identity/`](src/identity/) | test suites | `identity/tests.rs` |

## Public API

`identity::GitIdentity`,
`identity::load_git_identity`,
`identity::CAPSULE_SUPERVISOR_USER`,
`identity::host_uid`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-identity
cargo clippy -p jackin-runtime-identity --all-targets -- -D warnings
```
