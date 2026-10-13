# jackin-runtime-cleanup-dind-gc

Orphaned `DinD`
sidecar and
network garbage
collection for
cleanup.

## What this crate owns

- Enumeration
  (`dind_gc`):
  `DindInfo`,
  `collect_labeled_dind` —
  labeled `DinD`
  sidecar rows from
  Docker.
- Classification
  (`dind_gc`):
  `filter_orphaned_dind` —
  sidecars whose role
  container is gone.
- Sweep
  (`dind_gc`):
  `gc_orphaned_resources`,
  `gc_orphaned_networks`,
  `gc_orphaned_prewarm_dind` —
  best-effort removal of
  orphaned sidecars,
  cert volumes, role
  networks, and the
  stale prewarm sidecar.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`dind_gc.rs`](src/dind_gc.rs) | orphaned-resource GC | hub `cleanup` suite |

## Public API

`dind_gc::DindInfo`,
`dind_gc::collect_labeled_dind`,
`dind_gc::filter_orphaned_dind`,
`dind_gc::gc_orphaned_resources`,
`dind_gc::gc_orphaned_networks`,
`dind_gc::gc_orphaned_prewarm_dind`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-cleanup-dind-gc
cargo clippy -p jackin-runtime-cleanup-dind-gc --all-targets -- -D warnings
```
