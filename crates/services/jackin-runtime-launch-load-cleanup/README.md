# jackin-runtime-launch-load-cleanup

Launch teardown
coordinator and
atomic-write guard.

## What this crate owns

- Coordinator
  (`load_cleanup`):
  `LoadCleanup` —
  Docker resource
  teardown for failed
  or completed
  launches.
- Guard
  (`load_cleanup`):
  `write_if_changed_atomic` —
  temp+rename write
  that skips unchanged
  single-file bind
  mounts.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`load_cleanup.rs`](src/load_cleanup.rs) | coordinator + guard | — |

## Public API

`load_cleanup::LoadCleanup`,
`load_cleanup::write_if_changed_atomic`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-launch-load-cleanup
cargo clippy -p jackin-runtime-launch-load-cleanup --all-targets -- -D warnings
```
