# jackin-runtime-apple-container-attach

Jackin apple-container
interactive attach
step.

## What this crate owns

- Attach step
  (`attach`):
  `attach` —
  run `container
  exec -it` against
  the capsule binary
  with inherited stdio
  and reassert the
  alternate screen on
  return, shared by the
  apple-container
  `launch` and
  `reconnect` paths
  (S7 split 110).

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`attach.rs`](src/attach.rs) | attach step | hub `apple_container` paths (no dedicated suite) |

## Public API

`attach::attach`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-apple-container-attach
cargo clippy -p jackin-runtime-apple-container-attach --all-targets -- -D warnings
```
