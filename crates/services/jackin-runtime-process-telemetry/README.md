# jackin-runtime-process-telemetry

Telemetry-instrumented process
spawn/exec wrappers: every
`jackin-process` call carries an
OTLP operation guard with the
outcome matrix.

## What this crate owns

- Wrappers (`process_telemetry`):
  `spawn_sync`, `spawn_async`,
  `exec_sync`, `exec_async` with
  exit-code attrs and timeout /
  spawn-error outcomes.
- Guard (`process_telemetry`):
  `ChildOperation` span lifecycle
  for long-lived children
  (complete on status, success,
  timeout, failure, or drop).

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`process_telemetry.rs`](src/process_telemetry.rs) | wrappers + guard | `process_telemetry/tests/` |

## Public API

`process_telemetry::ChildOperation`,
`process_telemetry::spawn_sync`,
`process_telemetry::spawn_async`,
`process_telemetry::exec_sync`,
`process_telemetry::exec_async`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-process-telemetry
cargo clippy -p jackin-runtime-process-telemetry --all-targets -- -D warnings
```
