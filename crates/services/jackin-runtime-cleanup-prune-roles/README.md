# jackin-runtime-cleanup-prune-roles

Jackin runtime
cached role
repository pruning
for cleanup.

## What this crate owns

- Pruning
  (`prune_roles`):
  `prune_roles` —
  remove the cached
  role repositories
  behind the
  coordination gate,
  deleted through
  the shared
  prune-one-directory
  helper. Instance,
  cache, and home
  pruning stay in
  the hub
  (S7 split 103).

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`prune_roles.rs`](src/prune_roles.rs) | pruning | hub `cleanup` suite (`cleanup/tests/case_05.rs`) |

## Public API

`prune_roles::prune_roles`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-cleanup-prune-roles
cargo clippy -p jackin-runtime-cleanup-prune-roles --all-targets -- -D warnings
```
