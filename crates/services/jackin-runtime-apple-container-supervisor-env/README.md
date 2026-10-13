# jackin-runtime-apple-container-supervisor-env

Jackin apple-container
supervisor env
builder.

## What this crate owns

- Supervisor env
  pairs
  (`supervisor_env`):
  `apple_supervisor_env`
  — build the
  daemon-mode +
  supervisor-PID
  entries (plus the
  debug telemetry
  level when
  requested) that the
  `launch` path seeds
  into the container
  spec env (S7 split 114).

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`supervisor_env.rs`](src/supervisor_env.rs) | supervisor env pairs | hub `apple_container` launch path (no dedicated suite) |

## Public API

`supervisor_env::apple_supervisor_env`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-apple-container-supervisor-env
cargo clippy -p jackin-runtime-apple-container-supervisor-env --all-targets -- -D warnings
```
