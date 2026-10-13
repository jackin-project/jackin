# jackin-runtime-apple-container-wait

Jackin apple-container
capsule readiness
wait.

## What this crate owns

- Readiness wait
  (`wait`):
  `wait_for_capsule` —
  poll `container
  exec` until the
  capsule socket
  negotiates the
  protocol major,
  shared by the
  apple-container
  `launch` and
  `reconnect` paths
  (S7 split 111).

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`wait.rs`](src/wait.rs) | readiness wait | hub `apple_container` paths (no dedicated suite) |

## Public API

`wait::wait_for_capsule`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-apple-container-wait
cargo clippy -p jackin-runtime-apple-container-wait --all-targets -- -D warnings
```
