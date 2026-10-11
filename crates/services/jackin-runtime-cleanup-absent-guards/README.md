# jackin-runtime-cleanup-absent-guards

Absent-for-purge
guards refusing
local-state teardown
while Docker
resources exist.

## What this crate owns

- Guards
  (`absent_guards`):
  `ensure_role_resources_absent_for_purge` —
  refuse purge while
  the role container
  or DinD sidecar
  still exists;
  uninspectable state
  also refuses.
  Bulk prune stays
  in the hub
  (S7 split 100).

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`absent_guards.rs`](src/absent_guards.rs) | guards | hub `cleanup` suite |

## Public API

`absent_guards::ensure_role_resources_absent_for_purge`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-cleanup-absent-guards
cargo clippy -p jackin-runtime-cleanup-absent-guards --all-targets -- -D warnings
```
