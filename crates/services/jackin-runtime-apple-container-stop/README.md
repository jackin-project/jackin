# jackin-runtime-apple-container-stop

Jackin apple-container
container stop
helper.

## What this crate owns

- Container stop
  (`stop`):
  `stop_with` —
  stop a container
  through an injected
  client (S7 split 120).

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`stop.rs`](src/stop.rs) | Container stop helper | hub `backend` eject path (no dedicated suite) |

## Public API

`stop::stop_with`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-apple-container-stop
cargo clippy -p jackin-runtime-apple-container-stop --all-targets -- -D warnings
```
