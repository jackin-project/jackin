# jackin-runtime-apple-container-attach-outcome

Jackin apple-container
post-attach outcome
recording.

## What this crate owns

- Outcome recording
  (`attach_outcome`):
  `record_attach_outcome` —
  probe the container
  through the shared
  running-state probe
  and record whether a
  detached role is still
  running into the
  instance manifest,
  shared by the
  apple-container
  `launch` and
  `reconnect` paths
  (S7 split 109).

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`attach_outcome.rs`](src/attach_outcome.rs) | recording | hub `apple_container` paths (no dedicated suite) |

## Public API

`attach_outcome::record_attach_outcome`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-apple-container-attach-outcome
cargo clippy -p jackin-runtime-apple-container-attach-outcome --all-targets -- -D warnings
```
