# jackin-runtime-cleanup-prune-dir

Jackin runtime shared
prune-one-directory
helper for cleanup.

## What this crate owns

- Pruning
  (`prune_dir`):
  `prune_dir` —
  delete one directory
  through the
  owned-validated
  remover, reported
  through prune-output
  rows with failures
  recorded as cleanup
  failures. Role,
  cache, and instance
  pruning stay in the
  hub (S7 split 102).

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`prune_dir.rs`](src/prune_dir.rs) | pruning | hub `cleanup` suite (`cleanup/tests/case_04.rs`, `case_05.rs`) |

## Public API

`prune_dir::prune_dir`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-cleanup-prune-dir
cargo clippy -p jackin-runtime-cleanup-prune-dir --all-targets -- -D warnings
```
