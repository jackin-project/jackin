# jackin-instance

Role instance lifecycle: the instance index, the per-role state directory, auth API compatibility, and container naming. An "instance" is the on-disk + in-Docker state for a single running (or restorable) role session.

Auth provisioning lives in the sibling crates `jackin-instance-roles` (orchestration), `jackin-instance-agents` (per-agent provisioners), and `jackin-instance-credentials` (substrate); this crate re-exports their API unchanged.

## What this crate owns

- The instance index and lifecycle (`lib`, `tests`), role-state directory management, and container naming (`naming`).
- The instance's view of its manifest (`manifest`).

## Architecture tier and allowed dependencies

**L1 application.** Allowed workspace dependencies: `jackin-core`, `jackin-config`, `jackin-instance-roles`, `jackin-instance-agents`, `jackin-instance-credentials`. No presentation or runtime dependencies — instance lifecycle stays a domain/app concern above the leaf and below orchestration.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | instance index + lifecycle | — |
| [`manifest.rs`](src/manifest.rs) · [`manifest/`](src/manifest) | instance manifest view | [`tests.rs`](src/manifest/tests.rs) |
| [`naming.rs`](src/naming.rs) · [`naming/`](src/naming) | container/instance naming | [`tests.rs`](src/naming/tests.rs) |
| [`tests.rs`](src/tests.rs) | integration tests | — |

## Public API

Instance identity, the role-state directory contract, and naming used by `jackin-runtime`, `jackin-isolation`, and the host CLI. Naming is shared with the capsule side via `jackin-protocol`.

Typed errors: [`InstanceError`](../jackin-instance-credentials/src/error.rs) (index/auth join/manifest agent) and
[`SyncSourceValidationError`](../jackin-instance-credentials/src/error.rs) (sync-source folder checks).

## How to verify

```sh
cargo nextest run -p jackin-instance -p jackin-instance-roles -p jackin-instance-agents -p jackin-instance-credentials
cargo clippy -p jackin-instance --all-targets -- -D warnings
```
