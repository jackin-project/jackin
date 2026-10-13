# jackin-runtime-cleanup-prune-home

Jackin runtime home
directory pruning for
runtime cleanup.

## What this crate owns

- Pruning
  (`prune_home`):
  `prune_jackin_home` —
  remove the remaining
  runtime home state
  behind the
  coordination gate,
  reported through
  prune-output rows.
  Instance, role, and
  cache pruning stay
  in the hub
  (S7 split 101).

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`prune_home.rs`](src/prune_home.rs) | pruning | hub `cleanup` suite (`cleanup/tests/case_05.rs`) |

## Public API

`prune_home::prune_jackin_home`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-cleanup-prune-home
cargo clippy -p jackin-runtime-cleanup-prune-home --all-targets -- -D warnings
```
