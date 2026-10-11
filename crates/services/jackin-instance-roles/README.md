# jackin-instance-roles

Role-state orchestration for role instances: `RoleState` preparation dispatch across agents and bindings. Depends on `jackin-instance-agents` and `jackin-instance-credentials`.

## What this crate owns

- The `RoleState` record and preparation context (`role_state`).
- Preparation dispatch for agents and bindings (`prepare`, `provision`).

## Architecture tier and allowed dependencies

**L1 application.** Allowed workspace dependencies: `jackin-core`, `jackin-config`, `jackin-manifest`, `jackin-diagnostics`, `jackin-telemetry`, `jackin-instance-agents`, `jackin-instance-credentials`. No dependency back toward `jackin-instance`, which re-exports this API.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`role_state.rs`](src/role_state.rs) | `RoleState` record | [`auth_tests.rs`](src/auth_tests.rs) |
| [`prepare.rs`](src/prepare.rs) · [`provision.rs`](src/provision.rs) | preparation dispatch | [`tests.rs`](src/tests.rs) |

## Public API

`RoleState` preparation consumed by `jackin-runtime` and re-exported through `jackin-instance`.

## How to verify

```sh
cargo nextest run -p jackin-instance-roles
cargo clippy -p jackin-instance-roles --all-targets -- -D warnings
```
