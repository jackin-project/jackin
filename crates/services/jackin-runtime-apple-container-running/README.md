# jackin-runtime-apple-container-running

Jackin apple-container
running-state probe
for attach.

## What this crate owns

- Probing
  (`running`):
  `is_container_running` —
  report whether an
  apple/container
  container is running
  through the shared
  client listing,
  shared by the
  reconnect path and
  the post-attach
  outcome recording
  (S7 split 108).

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`running.rs`](src/running.rs) | probing | hub `apple_container` paths (no dedicated suite) |

## Public API

`running::is_container_running`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-apple-container-running
cargo clippy -p jackin-runtime-apple-container-running --all-targets -- -D warnings
```
