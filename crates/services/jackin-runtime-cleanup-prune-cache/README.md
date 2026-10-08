# jackin-runtime-cleanup-prune-cache

Jackin runtime
rebuildable shared
cache pruning
for cleanup.

## What this crate owns

- Pruning
  (`prune_cache`):
  `prune_cache` —
  remove the
  rebuildable shared
  cache behind the
  coordination gate,
  deleted through
  the shared
  prune-one-directory
  helper. Instance
  pruning stays in
  the hub
  (S7 split 104).

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`prune_cache.rs`](src/prune_cache.rs) | pruning | hub `cleanup` suite (`cleanup/tests/case_05.rs`) |

## Public API

`prune_cache::prune_cache`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-cleanup-prune-cache
cargo clippy -p jackin-runtime-cleanup-prune-cache --all-targets -- -D warnings
```
