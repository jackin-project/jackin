# jackin-runtime-cleanup-eject-resources

Jackin runtime
prevalidated Docker
resource ejection
for cleanup.

## What this crate owns

- Ejection
  (`eject_resources`):
  `eject_docker_role_with_resources` —
  remove the role and
  DinD containers,
  certs volume, and
  network for
  prevalidated
  handles, then the
  host-side socket
  dir. Shared by the
  hub eject and
  attach
  reconnect-lease
  flows (S7 split
  106).

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`eject_resources.rs`](src/eject_resources.rs) | ejection | hub `cleanup` suite (`cleanup/tests/case_01.rs`) |

## Public API

`eject_resources::eject_docker_role_with_resources`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-cleanup-eject-resources
cargo clippy -p jackin-runtime-cleanup-eject-resources --all-targets -- -D warnings
```
