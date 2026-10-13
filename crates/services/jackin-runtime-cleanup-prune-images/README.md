# jackin-runtime-cleanup-prune-images

Unused
jackin-managed Docker
image pruning for
runtime cleanup.

## What this crate owns

- Pruning
  (`prune_images`):
  `prune_images` —
  remove `jk_*` images
  no role container
  still references,
  best-effort per
  image with a
  removed/skipped/failed
  summary. Instance
  and role pruning
  stay in the hub
  (S7 split 99).

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`prune_images.rs`](src/prune_images.rs) | pruning | hub `cleanup` suite (`cleanup/tests/case_04.rs`) |

## Public API

`prune_images::prune_images`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-cleanup-prune-images
cargo clippy -p jackin-runtime-cleanup-prune-images --all-targets -- -D warnings
```
