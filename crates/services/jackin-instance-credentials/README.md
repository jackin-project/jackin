# jackin-instance-credentials

Credential substrate for role instance provisioning: outcome types, journaled auth-directory transactions, and secure path primitives. Leaf crate — no dependencies on the other instance crates.

## What this crate owns

- Provisioning outcome and token-source types (`outcomes`, `error`), including the GitHub auth context.
- The journaled auth directory (`auth_directory`): leases, locks, staging, and crash recovery.
- Secure path primitives (`paths`, `mounts`, `permissions`) and capture budgets (`limits`).

## Architecture tier and allowed dependencies

**L1 application.** Allowed workspace dependencies: `jackin-core`, `jackin-config`. No instance-sibling dependencies — this crate is the bottom of the instance stack.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`outcomes.rs`](src/outcomes.rs) · [`error.rs`](src/error.rs) | outcome/error types | — |
| [`auth_directory.rs`](src/auth_directory.rs) · [`auth_directory/`](src/auth_directory) | journaled auth dir | — |
| [`paths.rs`](src/paths.rs) · [`mounts.rs`](src/mounts.rs) · [`permissions.rs`](src/permissions.rs) | secure path primitives | — |
| [`limits.rs`](src/limits.rs) | capture budgets | — |

## Public API

Outcome types and auth-directory leases consumed by `jackin-instance-agents` and re-exported through `jackin-instance`.

Typed errors: [`InstanceError`](src/error.rs) (index/auth join/manifest agent) and
[`SyncSourceValidationError`](src/error.rs) (sync-source folder checks).

## How to verify

```sh
cargo nextest run -p jackin-instance-credentials
cargo clippy -p jackin-instance-credentials --all-targets -- -D warnings
```
